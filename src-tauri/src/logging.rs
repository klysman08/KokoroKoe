use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;

use crate::domain::AppError;

static LOGGING_INITIALIZED: OnceLock<()> = OnceLock::new();

pub(crate) fn init() {
    LOGGING_INITIALIZED.get_or_init(|| {
        let filter = if cfg!(debug_assertions) {
            EnvFilter::new("kokorokoe_lib=debug")
        } else {
            EnvFilter::new("kokorokoe_lib=info")
        };

        let _ = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_env_filter(filter)
            .with_target(false)
            .compact()
            .try_init();
    });
}

pub(crate) fn record_app_error(error: &AppError) {
    tracing::error!(
        code = %error.code,
        severity = ?error.severity,
        correlation_id = %error.correlation_id,
        retryable = error.retryable,
        "application operation failed"
    );
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use tracing_subscriber::fmt::MakeWriter;

    use super::record_app_error;
    use crate::domain::AppError;

    #[derive(Clone)]
    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    struct CaptureGuard(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for CapturedWriter {
        type Writer = CaptureGuard;

        fn make_writer(&'a self) -> Self::Writer {
            CaptureGuard(Arc::clone(&self.0))
        }
    }

    impl io::Write for CaptureGuard {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn tracing_records_only_the_sanitized_error_envelope() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_target(false)
            .with_writer(CapturedWriter(Arc::clone(&buffer)))
            .finish();
        let error = AppError::settings_unavailable(
            "secret-canary C:\\Users\\Example\\meeting.md Bearer private-token",
        );

        tracing::subscriber::with_default(subscriber, || record_app_error(&error));

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(output.contains("settings_unavailable"));
        assert!(output.contains(&error.correlation_id.to_string()));
        assert!(!output.contains("secret-canary"));
        assert!(!output.contains("meeting.md"));
        assert!(!output.contains("private-token"));
    }
}
