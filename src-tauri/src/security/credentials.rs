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
const OPENROUTER_VALIDATION_TARGET: &str = "KokoroKoe_OpenRouter_Validated_At";
const OPENROUTER_USERNAME: &str = "OpenRouter";

trait CredentialBackend: Send + Sync {
    fn read_secret(&self) -> Result<Option<Vec<u8>>, AppError>;
    fn write_secret(&self, secret: &[u8]) -> Result<(), AppError>;
    fn delete_secret(&self) -> Result<(), AppError>;
    fn read_validation(&self) -> Result<Option<String>, AppError>;
    fn write_validation(&self, validated_at: &str) -> Result<(), AppError>;
    fn delete_validation(&self) -> Result<(), AppError>;
}

#[derive(Clone)]
pub(crate) struct CredentialService {
    backend: std::sync::Arc<dyn CredentialBackend>,
}

impl CredentialService {
    pub(crate) fn open() -> Self {
        Self::with_backend(std::sync::Arc::new(WindowsCredentialBackend::new(
            OPENROUTER_TARGET,
            OPENROUTER_VALIDATION_TARGET,
        )))
    }

    fn with_backend(backend: std::sync::Arc<dyn CredentialBackend>) -> Self {
        Self { backend }
    }

    pub(crate) fn status(&self) -> Result<CredentialStatus, AppError> {
        let Some(secret) = self.backend.read_secret()? else {
            return Ok(CredentialStatus::configured(false));
        };
        drop(OpenRouterApiKey::from_bytes(secret).map_err(AppError::credential_error)?);
        match self.backend.read_validation()? {
            Some(validated_at) => CredentialStatus::validated(validated_at)
                .map_err(|_| AppError::credential_error("credential_status_unavailable")),
            None => Ok(CredentialStatus::configured(true)),
        }
    }

    pub(crate) fn set(&self, api_key: &OpenRouterApiKey) -> Result<CredentialStatus, AppError> {
        api_key.validate().map_err(AppError::credential_error)?;
        self.backend.write_secret(api_key.expose())?;
        self.backend.delete_validation()?;
        Ok(CredentialStatus::configured(true))
    }

    pub(crate) fn delete(&self) -> Result<CredentialStatus, AppError> {
        self.backend.delete_secret()?;
        self.backend.delete_validation()?;
        Ok(CredentialStatus::configured(false))
    }

    pub(crate) fn load_api_key(&self) -> Result<OpenRouterApiKey, AppError> {
        let bytes = self
            .backend
            .read_secret()?
            .ok_or_else(|| AppError::openrouter_error("openrouter_credential_missing"))?;
        OpenRouterApiKey::from_bytes(bytes).map_err(AppError::credential_error)
    }

    pub(crate) fn record_validation(&self, validated_at: &str) -> Result<(), AppError> {
        crate::domain::validate_rfc3339(validated_at)
            .map_err(|_| AppError::credential_error("credential_status_unavailable"))?;
        self.backend.write_validation(validated_at)
    }

    #[cfg(test)]
    pub(crate) fn in_memory(api_key: Option<&str>) -> Self {
        Self::with_backend(std::sync::Arc::new(MemoryCredentialBackend {
            secret: std::sync::Mutex::new(api_key.map(|value| value.as_bytes().to_vec())),
            validation: std::sync::Mutex::new(None),
        }))
    }
}

#[cfg(test)]
#[derive(Default)]
struct MemoryCredentialBackend {
    secret: std::sync::Mutex<Option<Vec<u8>>>,
    validation: std::sync::Mutex<Option<String>>,
}

#[cfg(test)]
impl CredentialBackend for MemoryCredentialBackend {
    fn read_secret(&self) -> Result<Option<Vec<u8>>, AppError> {
        Ok(self.secret.lock().unwrap().clone())
    }

    fn write_secret(&self, secret: &[u8]) -> Result<(), AppError> {
        self.secret.lock().unwrap().replace(secret.to_vec());
        Ok(())
    }

    fn delete_secret(&self) -> Result<(), AppError> {
        self.secret.lock().unwrap().take();
        Ok(())
    }

    fn read_validation(&self) -> Result<Option<String>, AppError> {
        Ok(self.validation.lock().unwrap().clone())
    }

    fn write_validation(&self, validated_at: &str) -> Result<(), AppError> {
        self.validation
            .lock()
            .unwrap()
            .replace(validated_at.to_owned());
        Ok(())
    }

    fn delete_validation(&self) -> Result<(), AppError> {
        self.validation.lock().unwrap().take();
        Ok(())
    }
}

struct WindowsCredentialBackend {
    secret_target: Vec<u16>,
    validation_target: Vec<u16>,
}

impl WindowsCredentialBackend {
    fn new(secret_target: &str, validation_target: &str) -> Self {
        Self {
            secret_target: secret_target.encode_utf16().chain(Some(0)).collect(),
            validation_target: validation_target.encode_utf16().chain(Some(0)).collect(),
        }
    }

    fn read_blob(&self, target: &[u16]) -> Result<Option<Vec<u8>>, AppError> {
        let mut raw = ptr::null_mut();
        // SAFETY: target is a stable NUL-terminated UTF-16 buffer and raw is an out pointer.
        let found = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
        if found != 0 {
            // SAFETY: a successful call returns a valid CREDENTIALW and blob for its size.
            let credential = unsafe { &*raw };
            let blob = if credential.CredentialBlobSize == 0 {
                Vec::new()
            } else {
                // SAFETY: the blob belongs to raw and remains valid until CredFree below.
                unsafe {
                    std::slice::from_raw_parts(
                        credential.CredentialBlob,
                        credential.CredentialBlobSize as usize,
                    )
                }
                .to_vec()
            };
            // SAFETY: a successful CredReadW returns one buffer owned by CredFree.
            unsafe { CredFree(raw.cast::<c_void>()) };
            return Ok(Some(blob));
        }
        // SAFETY: GetLastError immediately follows the failed Win32 call on this thread.
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            Ok(None)
        } else {
            Err(AppError::credential_error("credential_status_unavailable"))
        }
    }

    fn write_blob(&self, target: &[u16], value: &[u8]) -> Result<(), AppError> {
        let blob_size = u32::try_from(value.len())
            .map_err(|_| AppError::credential_error("credential_key_invalid"))?;
        let mut username: Vec<u16> = OPENROUTER_USERNAME.encode_utf16().chain(Some(0)).collect();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_ptr().cast_mut(),
            CredentialBlobSize: blob_size,
            CredentialBlob: value.as_ptr().cast_mut(),
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

    fn delete_blob(&self, target: &[u16]) -> Result<(), AppError> {
        // SAFETY: target is a stable NUL-terminated UTF-16 buffer; flags are reserved zero.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
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

impl CredentialBackend for WindowsCredentialBackend {
    fn read_secret(&self) -> Result<Option<Vec<u8>>, AppError> {
        self.read_blob(&self.secret_target)
    }

    fn write_secret(&self, secret: &[u8]) -> Result<(), AppError> {
        self.write_blob(&self.secret_target, secret)
    }

    fn delete_secret(&self) -> Result<(), AppError> {
        self.delete_blob(&self.secret_target)
    }

    fn read_validation(&self) -> Result<Option<String>, AppError> {
        let Some(bytes) = self.read_blob(&self.validation_target)? else {
            return Ok(None);
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| AppError::credential_error("credential_status_unavailable"))
    }

    fn write_validation(&self, validated_at: &str) -> Result<(), AppError> {
        self.write_blob(&self.validation_target, validated_at.as_bytes())
    }

    fn delete_validation(&self) -> Result<(), AppError> {
        self.delete_blob(&self.validation_target)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[derive(Default)]
    struct FakeBackend(MemoryCredentialBackend);

    impl CredentialBackend for FakeBackend {
        fn read_secret(&self) -> Result<Option<Vec<u8>>, AppError> {
            self.0.read_secret()
        }

        fn write_secret(&self, secret: &[u8]) -> Result<(), AppError> {
            self.0.write_secret(secret)
        }

        fn delete_secret(&self) -> Result<(), AppError> {
            self.0.delete_secret()
        }

        fn read_validation(&self) -> Result<Option<String>, AppError> {
            self.0.read_validation()
        }

        fn write_validation(&self, validated_at: &str) -> Result<(), AppError> {
            self.0.write_validation(validated_at)
        }

        fn delete_validation(&self) -> Result<(), AppError> {
            self.0.delete_validation()
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
            backend.0.secret.lock().unwrap().as_deref(),
            Some(&b"secret-canary-second-5678"[..])
        );
        let restarted = CredentialService::with_backend(backend);
        restarted.record_validation("2026-08-13T12:34:56Z").unwrap();
        assert_eq!(
            restarted.status().unwrap().validated_at.as_deref(),
            Some("2026-08-13T12:34:56Z")
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
        struct Cleanup(String, String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let backend = WindowsCredentialBackend::new(&self.0, &self.1);
                let _ = backend.delete_secret();
                let _ = backend.delete_validation();
            }
        }

        let target = format!("KokoroKoe_P5_001_Test_{}", uuid::Uuid::new_v4());
        let validation_target = format!("{target}_Validation");
        let _cleanup = Cleanup(target.clone(), validation_target.clone());
        let backend = WindowsCredentialBackend::new(&target, &validation_target);
        let service = CredentialService::with_backend(Arc::new(backend));
        assert!(!service.status().unwrap().configured);
        assert!(
            service
                .set(&secret("secret-canary-os-store-9012"))
                .unwrap()
                .configured
        );
        let restarted = CredentialService::with_backend(Arc::new(WindowsCredentialBackend::new(
            &target,
            &validation_target,
        )));
        assert!(restarted.status().unwrap().configured);
        restarted.record_validation("2026-08-13T12:34:56Z").unwrap();
        assert_eq!(
            restarted.status().unwrap().validated_at.as_deref(),
            Some("2026-08-13T12:34:56Z")
        );
        assert!(!restarted.delete().unwrap().configured);
    }
}
