use serde::Deserialize;
use tauri::WebviewWindow;

use crate::{
    domain::{AppError, CommandError},
    logging,
    security::authorize_main_window,
};

const REPOSITORY_URL: &str = "https://github.com/klysman08/KokoroKoe";
const AUTHOR_URL: &str = "https://github.com/klysman08";

/// A page the sidebar may open in the default browser.
///
/// The frontend names a page, never a URL: the addresses are fixed here, so
/// this command cannot be turned into a way to open arbitrary links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ExternalPage {
    Repository,
    Author,
}

impl ExternalPage {
    fn url(self) -> &'static str {
        match self {
            Self::Repository => REPOSITORY_URL,
            Self::Author => AUTHOR_URL,
        }
    }
}

/// Opens the project's repository or its author's profile in the default
/// browser. Exact-`main`; it sends nothing and reads nothing.
#[tauri::command]
pub(crate) async fn open_external_page<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    page: ExternalPage,
) -> Result<(), CommandError> {
    authorize_main_window(webview_window.label()).map_err(record_error)?;
    tauri::async_runtime::spawn_blocking(move || open_in_browser(page.url()))
        .await
        .map_err(|_| record_error(AppError::external_page_open_failed()))?
        .map_err(record_error)
}

#[cfg(windows)]
fn open_in_browser(url: &'static str) -> Result<(), AppError> {
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|_| AppError::external_page_open_failed())
}

#[cfg(not(windows))]
fn open_in_browser(_url: &'static str) -> Result<(), AppError> {
    Err(AppError::external_page_open_failed())
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use super::ExternalPage;

    #[test]
    fn only_the_two_fixed_pages_can_be_named() {
        let page: ExternalPage = serde_json::from_str("\"repository\"").unwrap();
        assert_eq!(page.url(), "https://github.com/klysman08/KokoroKoe");
        let page: ExternalPage = serde_json::from_str("\"author\"").unwrap();
        assert_eq!(page.url(), "https://github.com/klysman08");

        for rejected in [
            "\"https://example.com\"",
            "\"Repository\"",
            "\"\"",
            "{\"url\":\"https://example.com\"}",
        ] {
            assert!(serde_json::from_str::<ExternalPage>(rejected).is_err());
        }
    }
}
