use crate::domain::AppError;

const MAIN_WINDOW_LABEL: &str = "main";

pub(crate) fn authorize_main_window(window_label: &str) -> Result<(), AppError> {
    if window_label == MAIN_WINDOW_LABEL {
        Ok(())
    } else {
        Err(AppError::command_not_authorized())
    }
}

#[cfg(test)]
mod tests {
    use super::authorize_main_window;

    #[test]
    fn only_the_exact_main_window_label_is_authorized() {
        assert!(authorize_main_window("main").is_ok());

        for label in ["Main", "main-settings", "transcript", "insights", "unknown"] {
            let error = authorize_main_window(label).expect_err("must reject other windows");
            assert_eq!(error.code, "command_not_authorized");
            assert!(!error.retryable);
        }
    }
}
