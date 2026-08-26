use crate::domain::AppError;

const MAIN_WINDOW_LABEL: &str = "main";
const TRANSCRIPT_WINDOW_LABEL: &str = "transcript";

pub(crate) fn authorize_main_window(window_label: &str) -> Result<(), AppError> {
    if window_label == MAIN_WINDOW_LABEL {
        Ok(())
    } else {
        Err(AppError::command_not_authorized())
    }
}

/// The one command that is not exact-`main`, and it is exact-`transcript`.
///
/// The detached transcript window exists to be watched while the main window is
/// hidden, so marking a moment from it cannot be routed through the main
/// window: that window may not be there. ADR 0010 records the decision and the
/// shape that makes it acceptable — the command this guards can flip an
/// importance flag and nothing else, so it carries no way to write text into
/// the record.
///
/// The main window is deliberately *not* admitted here. It already marks
/// through `annotate_transcript_segment`, and holding this rule to the single
/// window whose capability file grants the command keeps the two layers saying
/// exactly the same thing, which is the property ADR 0008 relies on.
pub(crate) fn authorize_transcript_window(window_label: &str) -> Result<(), AppError> {
    if window_label == TRANSCRIPT_WINDOW_LABEL {
        Ok(())
    } else {
        Err(AppError::command_not_authorized())
    }
}

#[cfg(test)]
mod tests {
    use super::{authorize_main_window, authorize_transcript_window};

    #[test]
    fn only_the_exact_main_window_label_is_authorized() {
        assert!(authorize_main_window("main").is_ok());

        for label in ["Main", "main-settings", "transcript", "insights", "unknown"] {
            let error = authorize_main_window(label).expect_err("must reject other windows");
            assert_eq!(error.code, "command_not_authorized");
            assert!(!error.retryable);
        }
    }

    /// The widening is one exact label and stops there. A near-miss is still a
    /// different window, the insights window is not part of it, and the main
    /// window is not either — it marks through its own command.
    #[test]
    fn marking_admits_the_transcript_window_and_no_other() {
        assert!(authorize_transcript_window("transcript").is_ok());

        for label in [
            "main",
            "Transcript",
            "transcript ",
            "transcript-1",
            "transcripts",
            "insights",
            "unknown",
            "",
        ] {
            let error = authorize_transcript_window(label).expect_err("must reject other windows");
            assert_eq!(error.code, "command_not_authorized");
            assert!(!error.retryable);
        }
    }
}
