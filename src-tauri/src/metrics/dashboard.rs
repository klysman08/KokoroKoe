use std::{collections::HashMap, path::PathBuf};

use crate::{
    domain::{
        AppError, DASHBOARD_RANKING_LIMIT, DashboardDiskUsage, DashboardMetrics,
        DashboardMetricsQuery, MetricsWindow, PageRequest, ProjectSessionCount, Session,
        TranscriptionModelSessionCount, parse_unix_ms,
    },
    persistence::{SessionCatalog, SettingsService, TranscriptSearchCatalog},
};

use super::{
    disk::measure_directory,
    store::{MetricsError, MetricsStore},
};

const SESSION_PAGE_LIMIT: u16 = 100;
/// 50,000 sessions. A workspace past that still gets a dashboard; it counts
/// what it read and says nothing about the rest.
const MAX_SESSION_PAGES: usize = 500;

/// Reads the local metrics the Home dashboard shows.
///
/// Everything here is read-only. Session counts, models, and speech time come
/// from the rebuildable indexes of the workspace; token use, cost, insight
/// counts, and errors come from the local usage ledger, because no Markdown
/// document records them. Nothing is sent anywhere.
#[derive(Clone)]
pub(crate) struct DashboardService {
    settings: SettingsService,
    app_data_directory: PathBuf,
    metrics: MetricsStore,
}

struct WorkspaceFacts {
    sessions: Vec<Session>,
    speech_ms: HashMap<String, u64>,
    workspace: PathBuf,
}

impl DashboardService {
    pub(crate) const fn new(
        settings: SettingsService,
        app_data_directory: PathBuf,
        metrics: MetricsStore,
    ) -> Self {
        Self {
            settings,
            app_data_directory,
            metrics,
        }
    }

    pub(crate) fn get_metrics(
        &self,
        query: &DashboardMetricsQuery,
    ) -> Result<DashboardMetrics, AppError> {
        let window = query.window().map_err(AppError::dashboard_error)?;
        let facts = self.settings.with_workspace_operation(|workspace| {
            let catalog = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::dashboard_error(error.code))?;
            let sessions = all_sessions(&catalog)?;
            let speech_ms =
                TranscriptSearchCatalog::open(workspace, self.app_data_directory.clone())
                    .and_then(|catalog| catalog.speech_ms_by_session())
                    .map_err(|error| AppError::dashboard_error(error.code))?;
            Ok(WorkspaceFacts {
                sessions,
                speech_ms,
                workspace: workspace.to_path_buf(),
            })
        })?;
        // Measured outside the workspace lock: a large folder takes a moment to
        // walk, and nothing else should wait on a dashboard for it.
        let workspace_disk = measure_directory(&facts.workspace);
        let model_disk = measure_directory(&self.app_data_directory.join("models"));
        let usage = self
            .metrics
            .usage_summary(&window)
            .map_err(map_metrics_error)?;
        let recent_errors = self
            .metrics
            .recent_errors(&window)
            .map_err(map_metrics_error)?;

        let selected = facts
            .sessions
            .iter()
            .filter(|session| in_window(session, &window))
            .collect::<Vec<_>>();
        let transcribed_ms = selected
            .iter()
            .filter_map(|session| facts.speech_ms.get(&session.id.as_uuid().to_string()))
            .fold(0_u64, |total, value| total.saturating_add(*value));
        let projects = ranked(selected.iter().map(|session| session.project_id))
            .into_iter()
            .map(|(project_id, sessions)| ProjectSessionCount {
                project_id,
                sessions,
            })
            .collect();
        let transcription_models = ranked(
            selected
                .iter()
                .map(|session| session.transcription_model_id.clone()),
        )
        .into_iter()
        .map(|(model_id, sessions)| TranscriptionModelSessionCount { model_id, sessions })
        .collect();

        let metrics = DashboardMetrics {
            schema_version: 1,
            sessions: u32::try_from(selected.len()).unwrap_or(u32::MAX),
            transcribed_ms,
            projects,
            transcription_models,
            llm_models: usage
                .models
                .into_iter()
                .take(DASHBOARD_RANKING_LIMIT)
                .collect(),
            llm_requests: usage.requests,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            actual_cost_usd: usage.actual_cost_usd,
            insights_generated: usage.insights_generated,
            usage_recorded_since: usage.recorded_since,
            disk: DashboardDiskUsage {
                workspace_bytes: workspace_disk.bytes,
                trash_bytes: workspace_disk.trash_bytes,
                model_bytes: model_disk.bytes,
                complete: workspace_disk.complete && model_disk.complete,
            },
            recent_errors,
        };
        metrics.validate().map_err(AppError::dashboard_error)?;
        Ok(metrics)
    }
}

fn all_sessions(catalog: &SessionCatalog) -> Result<Vec<Session>, AppError> {
    // A page cursor is bound to the index generation, so a rebuild between two
    // pages invalidates it. Starting again once is enough: a second rebuild in
    // the same instant means something else is rewriting the index, and the
    // honest answer is to say so rather than loop.
    for _ in 0..2 {
        match read_every_page(catalog) {
            Err(error) if error.code == "session_page_stale" => {}
            result => return result.map_err(|error| AppError::dashboard_error(error.code)),
        }
    }
    Err(AppError::dashboard_error("session_page_stale"))
}

fn read_every_page(
    catalog: &SessionCatalog,
) -> Result<Vec<Session>, crate::persistence::SessionStoreError> {
    let mut sessions = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_SESSION_PAGES {
        let page = catalog.list_sessions(&PageRequest {
            cursor,
            limit: SESSION_PAGE_LIMIT,
        })?;
        sessions.extend(page.items);
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(sessions)
}

/// A session belongs to the period it was created in, and to its project.
fn in_window(session: &Session, window: &MetricsWindow) -> bool {
    window
        .project_id
        .is_none_or(|project_id| session.project_id == project_id)
        && parse_unix_ms(&session.created_at).is_ok_and(|created| window.contains(created))
}

/// Counts each key and keeps the most frequent, ties broken by key so the
/// order is stable between two reads of the same data.
fn ranked<K: Ord + std::hash::Hash + Clone>(keys: impl Iterator<Item = K>) -> Vec<(K, u32)> {
    let mut counts = HashMap::<K, u32>::new();
    for key in keys {
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left_key, left), (right_key, right)| {
        right.cmp(left).then_with(|| left_key.cmp(right_key))
    });
    ranked.truncate(DASHBOARD_RANKING_LIMIT);
    ranked
}

fn map_metrics_error(error: MetricsError) -> AppError {
    AppError::dashboard_error(error.code)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        domain::{CreateProjectInput, RequestId},
        persistence::{ProjectService, SettingsService},
        prompts::PromptPurpose,
    };

    use super::super::store::UsageRecord;
    use super::*;

    fn project_input() -> CreateProjectInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-management-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["createInput"].clone()).unwrap()
    }

    fn all_time() -> DashboardMetricsQuery {
        serde_json::from_str("{}").unwrap()
    }

    #[test]
    fn an_empty_workspace_reads_as_zero_rather_than_failing() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let service = DashboardService::new(
            settings,
            app_data.path().to_path_buf(),
            MetricsStore::new(app_data.path().to_path_buf()),
        );

        let metrics = service.get_metrics(&all_time()).unwrap();
        assert_eq!(metrics.sessions, 0);
        assert_eq!(metrics.transcribed_ms, 0);
        assert_eq!(metrics.llm_requests, 0);
        assert_eq!(metrics.actual_cost_usd, "0.000000000000");
        assert!(metrics.recent_errors.is_empty());
    }

    /// The dashboard reads the same files every other screen does, and the
    /// ledger it reads is the one an OpenRouter call writes.
    #[test]
    fn usage_and_projects_reach_the_dashboard_and_honor_the_project_filter() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
        let project = projects.create_project(project_input()).unwrap();
        let metrics = MetricsStore::new(app_data.path().to_path_buf());
        let session_id =
            serde_json::from_value(serde_json::Value::String(uuid::Uuid::new_v4().to_string()))
                .unwrap();
        metrics
            .record_usage(&UsageRecord {
                request_id: RequestId::new(),
                project_id: project.id,
                session_id,
                purpose: PromptPurpose::Summary,
                model_id: "model/a",
                input_tokens: 10,
                output_tokens: 5,
                actual_cost_usd: "0.010000000000",
            })
            .unwrap();
        metrics
            .record_error(&AppError::workspace_error("workspace_unavailable"))
            .unwrap();
        let service =
            DashboardService::new(settings.clone(), app_data.path().to_path_buf(), metrics);

        let everything = service.get_metrics(&all_time()).unwrap();
        assert_eq!(everything.llm_requests, 1);
        assert_eq!(everything.llm_models[0].model_id, "model/a");
        assert_eq!(everything.recent_errors.len(), 1);
        let root = settings.get_settings().unwrap();
        assert!(
            everything.disk.workspace_bytes > 0,
            "{}",
            Path::new(root.workspace_path()).display()
        );

        let other_project: DashboardMetricsQuery = serde_json::from_value(serde_json::json!({
            "projectId": uuid::Uuid::new_v4().to_string()
        }))
        .unwrap();
        let scoped = service.get_metrics(&other_project).unwrap();
        assert_eq!(scoped.llm_requests, 0);
        assert_eq!(scoped.actual_cost_usd, "0.000000000000");
    }

    #[test]
    fn rankings_are_frequent_first_and_stable() {
        let ranked = ranked(["b", "a", "b", "c", "a", "b"].into_iter());
        assert_eq!(ranked, vec![("b", 3), ("a", 2), ("c", 1)]);
    }
}
