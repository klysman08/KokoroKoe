use std::{ffi::c_void, ptr};

use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, GetLastError},
    Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    },
};

use crate::domain::{AppError, CredentialStatus, OpenRouterApiKey};

const OPENROUTER_TARGET: &str = "KokoroKoe_OpenRouter_API_Key";
const OPENROUTER_USERNAME: &str = "OpenRouter";

trait CredentialBackend: Send + Sync {
    fn configured(&self) -> Result<bool, AppError>;
    fn write(&self, secret: &[u8]) -> Result<(), AppError>;
    fn delete(&self) -> Result<(), AppError>;
}

#[derive(Clone)]
pub(crate) struct CredentialService {
    backend: std::sync::Arc<dyn CredentialBackend>,
}

impl CredentialService {
    pub(crate) fn open() -> Self {
        Self::with_backend(std::sync::Arc::new(WindowsCredentialBackend::new(
            OPENROUTER_TARGET,
        )))
    }

    fn with_backend(backend: std::sync::Arc<dyn CredentialBackend>) -> Self {
        Self { backend }
    }

    pub(crate) fn status(&self) -> Result<CredentialStatus, AppError> {
        self.backend.configured().map(CredentialStatus::configured)
    }

    pub(crate) fn set(&self, api_key: &OpenRouterApiKey) -> Result<CredentialStatus, AppError> {
        api_key.validate().map_err(AppError::credential_error)?;
        self.backend.write(api_key.expose())?;
        Ok(CredentialStatus::configured(true))
    }

    pub(crate) fn delete(&self) -> Result<CredentialStatus, AppError> {
        self.backend.delete()?;
        Ok(CredentialStatus::configured(false))
    }
}

struct WindowsCredentialBackend {
    target: Vec<u16>,
}

impl WindowsCredentialBackend {
    fn new(target: &str) -> Self {
        Self {
            target: target.encode_utf16().chain(Some(0)).collect(),
        }
    }
}

impl CredentialBackend for WindowsCredentialBackend {
    fn configured(&self) -> Result<bool, AppError> {
        let mut raw = ptr::null_mut();
        // SAFETY: target is a stable NUL-terminated UTF-16 buffer and raw is an out pointer.
        let found = unsafe { CredReadW(self.target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
        if found != 0 {
            // SAFETY: a successful CredReadW returns one buffer owned by CredFree.
            unsafe { CredFree(raw.cast::<c_void>()) };
            return Ok(true);
        }
        // SAFETY: GetLastError immediately follows the failed Win32 call on this thread.
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            Ok(false)
        } else {
            Err(AppError::credential_error("credential_status_unavailable"))
        }
    }

    fn write(&self, secret: &[u8]) -> Result<(), AppError> {
        let blob_size = u32::try_from(secret.len())
            .map_err(|_| AppError::credential_error("credential_key_invalid"))?;
        let mut username: Vec<u16> = OPENROUTER_USERNAME.encode_utf16().chain(Some(0)).collect();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: self.target.as_ptr().cast_mut(),
            CredentialBlobSize: blob_size,
            CredentialBlob: secret.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: username.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: every pointer remains valid for the call and lengths match their buffers.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(AppError::credential_error("credential_store_unavailable"));
        }
        Ok(())
    }

    fn delete(&self) -> Result<(), AppError> {
        // SAFETY: target is a stable NUL-terminated UTF-16 buffer; flags are reserved zero.
        if unsafe { CredDeleteW(self.target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
            return Ok(());
        }
        // SAFETY: GetLastError immediately follows the failed Win32 call on this thread.
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(AppError::credential_error("credential_delete_unavailable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeBackend(Mutex<Option<Vec<u8>>>);

    impl CredentialBackend for FakeBackend {
        fn configured(&self) -> Result<bool, AppError> {
            Ok(self.0.lock().unwrap().is_some())
        }

        fn write(&self, secret: &[u8]) -> Result<(), AppError> {
            self.0.lock().unwrap().replace(secret.to_vec());
            Ok(())
        }

        fn delete(&self) -> Result<(), AppError> {
            self.0.lock().unwrap().take();
            Ok(())
        }
    }

    fn secret(value: &str) -> OpenRouterApiKey {
        serde_json::from_value(serde_json::json!(value)).unwrap()
    }

    #[test]
    fn status_set_replace_delete_and_restart_expose_no_secret() {
        let backend = Arc::new(FakeBackend::default());
        let service = CredentialService::with_backend(backend.clone());
        assert_eq!(
            service.status().unwrap(),
            CredentialStatus::configured(false)
        );
        let status = service.set(&secret("secret-canary-first-1234")).unwrap();
        assert_eq!(status, CredentialStatus::configured(true));
        assert!(!serde_json::to_string(&status).unwrap().contains("canary"));
        service.set(&secret("secret-canary-second-5678")).unwrap();
        assert_eq!(
            backend.0.lock().unwrap().as_deref(),
            Some(&b"secret-canary-second-5678"[..])
        );
        let restarted = CredentialService::with_backend(backend);
        assert_eq!(
            restarted.status().unwrap(),
            CredentialStatus::configured(true)
        );
        assert_eq!(
            restarted.delete().unwrap(),
            CredentialStatus::configured(false)
        );
        assert_eq!(
            restarted.delete().unwrap(),
            CredentialStatus::configured(false)
        );
    }

    #[test]
    fn explicit_windows_credential_manager_round_trip() {
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = WindowsCredentialBackend::new(&self.0).delete();
            }
        }

        let target = format!("KokoroKoe_P5_001_Test_{}", uuid::Uuid::new_v4());
        let _cleanup = Cleanup(target.clone());
        let backend = WindowsCredentialBackend::new(&target);
        let service = CredentialService::with_backend(Arc::new(backend));
        assert!(!service.status().unwrap().configured);
        assert!(
            service
                .set(&secret("secret-canary-os-store-9012"))
                .unwrap()
                .configured
        );
        let restarted =
            CredentialService::with_backend(Arc::new(WindowsCredentialBackend::new(&target)));
        assert!(restarted.status().unwrap().configured);
        assert!(!restarted.delete().unwrap().configured);
    }
}
