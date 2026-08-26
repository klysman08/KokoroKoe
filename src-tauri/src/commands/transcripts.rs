use tauri::{State, WebviewWindow};

use crate::{
    domain::{
        AnnotateTranscriptSegmentRequest, AppError, CommandError, MarkTranscriptSegmentRequest,
        ProjectId, SessionId, TranscriptBookmarkList, TranscriptBookmarkListRequest,
        TranscriptPage, TranscriptPageRequest, TranscriptSearchPageView, TranscriptSearchQuery,
        TranscriptSegmentHistoryRequest, TranscriptSegmentHistoryView, TranscriptSegmentView,
    },
    logging,
    persistence::TranscriptService,
    security::{authorize_main_window, authorize_transcript_window},
};

#[tauri::command]
pub(crate) async fn get_transcript_page<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    project_id: ProjectId,
    session_id: SessionId,
    cursor: Option<String>,
    limit: u16,
) -> Result<TranscriptPage, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        service.get_transcript_page(TranscriptPageRequest {
            project_id,
            session_id,
            cursor,
            limit,
        })
    })
    .await
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn search_transcript<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    project_id: ProjectId,
    session_id: Option<SessionId>,
    query: String,
    cursor: Option<String>,
    limit: u16,
) -> Result<TranscriptSearchPageView, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        service.search_transcript(TranscriptSearchQuery {
            project_id,
            session_id,
            query,
            cursor,
            limit,
        })
    })
    .await
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn annotate_transcript_segment<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    request: AnnotateTranscriptSegmentRequest,
) -> Result<TranscriptSegmentView, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.annotate_segment(request))
        .await
        .map_err(record_error)
}

/// Reads how one segment came to read as it does. Nothing is written.
#[tauri::command]
pub(crate) async fn get_transcript_segment_history<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    request: TranscriptSegmentHistoryRequest,
) -> Result<TranscriptSegmentHistoryView, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.get_segment_history(request))
        .await
        .map_err(record_error)
}

/// Every place in one Session the reader asked to return to. Nothing is written.
#[tauri::command]
pub(crate) async fn list_transcript_bookmarks<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    request: TranscriptBookmarkListRequest,
) -> Result<TranscriptBookmarkList, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.list_bookmarks(request))
        .await
        .map_err(record_error)
}

/// Flags one finalized segment. The only command a detached window may call.
///
/// It takes a flag rather than an annotation, so there is no shape of request
/// through which the transcript window could write text into the meeting
/// record. ADR 0010 records why the window holds this one and nothing else.
#[tauri::command]
pub(crate) async fn mark_transcript_segment<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    request: MarkTranscriptSegmentRequest,
) -> Result<TranscriptSegmentView, CommandError> {
    let service = authorized_for_marking(webview_window.label(), || state.inner().clone())
        .map_err(record_error)?;
    run_blocking(move || {
        service.annotate_segment(AnnotateTranscriptSegmentRequest {
            project_id: request.project_id,
            session_id: request.session_id,
            segment_id: request.segment_id,
            annotation: request.annotation(),
        })
    })
    .await
    .map_err(record_error)
}

fn authorized<T>(label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(label)?;
    Ok(operation())
}

fn authorized_for_marking<T>(label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_transcript_window(label)?;
    Ok(operation())
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::transcript_worker_failed())?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_transcript_command_authorizes_before_service_access() {
        let side_effects = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let result = super::authorized(label, || side_effects.fetch_add(1, Ordering::SeqCst));
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || ()).is_ok());
    }

    /// Marking is the one command that is not exact-`main`, and it is
    /// exact-`transcript`. It admits one window, and the widening does not
    /// reach any other command.
    #[test]
    fn marking_is_reachable_from_the_transcript_window_and_nowhere_else() {
        let side_effects = AtomicUsize::new(0);
        for label in ["main", "insights", "Transcript", "transcript-1", "unknown"] {
            let result = super::authorized_for_marking(label, || {
                side_effects.fetch_add(1, Ordering::SeqCst)
            });
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert!(super::authorized_for_marking("transcript", || ()).is_ok());
        // The widening does not leak into any other transcript command.
        assert_eq!(
            super::authorized("transcript", || ()).unwrap_err().code,
            "command_not_authorized"
        );
    }

    /// The request type is the boundary: there is no field for text, so a
    /// window holding this command cannot rewrite what the meeting recorded.
    #[test]
    fn a_marking_request_cannot_carry_text() {
        let rejected = serde_json::json!({
            "projectId": "11111111-1111-4111-8111-111111111111",
            "sessionId": "22222222-2222-4222-8222-222222222222",
            "segmentId": "33333333-3333-4333-8333-333333333333",
            "important": true,
            "text": "an injected rewrite",
        });
        assert!(
            serde_json::from_value::<crate::domain::MarkTranscriptSegmentRequest>(rejected)
                .is_err()
        );
    }
}
