use serde::Serialize;

use crate::domain::{Project, Session};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// P4-001 freezes this pure contract before a later task wires filesystem writes.
#[allow(dead_code)]
pub(crate) struct PortableFolderContract {
    pub(crate) project_directory: String,
    pub(crate) project_document: String,
    pub(crate) presets_directory: String,
    pub(crate) sessions_directory: String,
    pub(crate) session_directory: String,
    pub(crate) session_document: String,
    pub(crate) transcript_document: String,
    pub(crate) summary_document: String,
    pub(crate) insights_document: String,
    pub(crate) actions_document: String,
    pub(crate) questions_document: String,
    pub(crate) recovery_journal: String,
    pub(crate) audio_directory: String,
}

#[allow(dead_code)]
impl PortableFolderContract {
    pub(crate) fn for_records(project: &Project, session: &Session) -> Result<Self, &'static str> {
        project.validate()?;
        session.validate()?;
        if session.project_id != project.id {
            return Err("session_project_identity_mismatch");
        }

        let project_directory = format!("projects/{}", project.folder_name);
        let sessions_directory = format!("{project_directory}/sessions");
        let session_directory = format!("{sessions_directory}/{}", session.folder_name);
        Ok(Self {
            project_document: format!("{project_directory}/project.md"),
            presets_directory: format!("{project_directory}/presets"),
            sessions_directory,
            session_document: format!("{session_directory}/session.md"),
            transcript_document: format!("{session_directory}/transcript.md"),
            summary_document: format!("{session_directory}/summary.md"),
            insights_document: format!("{session_directory}/insights.md"),
            actions_document: format!("{session_directory}/actions.md"),
            questions_document: format!("{session_directory}/questions.md"),
            recovery_journal: format!("{session_directory}/recovery.journal"),
            audio_directory: format!("{session_directory}/audio"),
            project_directory,
            session_directory,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{Project, Session};

    use super::PortableFolderContract;

    fn records() -> (Project, Session) {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        (
            serde_json::from_value(fixture["project"].clone()).unwrap(),
            serde_json::from_value(fixture["session"].clone()).unwrap(),
        )
    }

    #[test]
    fn layout_contains_only_fixed_relative_descendants() {
        let (project, session) = records();
        let serialized =
            serde_json::to_value(PortableFolderContract::for_records(&project, &session).unwrap())
                .unwrap();
        for path in serialized.as_object().unwrap().values() {
            let path = path.as_str().unwrap();
            assert!(path.starts_with("projects/"));
            assert!(!path.contains(".."));
            assert!(!path.contains('\\'));
            assert!(!path.contains(':'));
        }
    }

    #[test]
    fn layout_rejects_a_session_from_another_project() {
        let (project, session) = records();
        let mut value = serde_json::to_value(session).unwrap();
        value["projectId"] = serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        let other: Session = serde_json::from_value(value).unwrap();

        assert_eq!(
            PortableFolderContract::for_records(&project, &other),
            Err("session_project_identity_mismatch")
        );
    }
}
