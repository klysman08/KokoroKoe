// P4-006 freezes the Rust-owned session snapshot boundary before a later task adds a
// product session service/command. Keep the implementation compiled and tested now.
#![allow(dead_code)]

use std::{
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

use crate::domain::{Project, Session};

use super::{
    layout::PortableFolderContract,
    project_store::{PinnedDirectory, ProjectLocator, ProjectStore, ProjectStoreError},
};

const TEMP_SESSION_DOCUMENT: &str = ".session.md.tmp";
const MAX_SESSION_DOCUMENT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionStoreError {
    pub(crate) code: &'static str,
}

impl SessionStoreError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for SessionStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for SessionStoreError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionCreateReceipt {
    pub(crate) relative_document: String,
    pub(crate) bytes_written: u64,
}

pub(crate) struct SessionStore {
    projects: ProjectStore,
}

impl SessionStore {
    pub(crate) fn open(workspace_path: &Path) -> Result<Self, SessionStoreError> {
        let projects = ProjectStore::open(workspace_path).map_err(map_workspace_error)?;
        Ok(Self { projects })
    }

    pub(crate) fn create_session(
        &self,
        project: &Project,
        session: &Session,
    ) -> Result<SessionCreateReceipt, SessionStoreError> {
        self.create_session_with_fault(project, session, None)
    }

    fn create_session_with_fault(
        &self,
        project: &Project,
        session: &Session,
        fault: Option<CreateFault>,
    ) -> Result<SessionCreateReceipt, SessionStoreError> {
        project
            .validate()
            .map_err(|_| SessionStoreError::new("session_project_invalid"))?;
        session
            .validate()
            .map_err(|_| SessionStoreError::new("session_contract_invalid"))?;
        let locator = ProjectLocator::from_project(project).map_err(map_project_error)?;

        // Pin the project before reading its authoritative snapshot. The no-delete-share
        // handle prevents a directory substitution between validation and session publish.
        let pinned_project = self
            .projects
            .open_existing_project(&locator)
            .map_err(map_project_error)?;
        let authoritative = self
            .projects
            .read_project(&locator)
            .map_err(map_project_error)?
            .project;
        let layout = PortableFolderContract::for_records(&authoritative, session)
            .map_err(map_contract_error)?;
        pinned_project.revalidate().map_err(map_path_error)?;

        let sessions_path = pinned_project.path().join("sessions");
        let created_sessions = create_directory(&sessions_path)?;
        let sessions = match PinnedDirectory::open(&sessions_path) {
            Ok(directory) => directory,
            Err(error) => {
                if created_sessions {
                    let _ = fs::remove_dir(&sessions_path);
                }
                return Err(map_path_error(error));
            }
        };
        if let Err(error) = pinned_project.revalidate().map_err(map_path_error) {
            drop(sessions);
            if created_sessions {
                let _ = fs::remove_dir(&sessions_path);
            }
            return Err(error);
        }

        let session_path = sessions_path.join(&session.folder_name);
        match fs::create_dir(&session_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                drop(sessions);
                if created_sessions {
                    let _ = fs::remove_dir(&sessions_path);
                }
                return Err(SessionStoreError::new("session_already_exists"));
            }
            Err(_) => {
                drop(sessions);
                if created_sessions {
                    let _ = fs::remove_dir(&sessions_path);
                }
                return Err(SessionStoreError::new("session_directory_create_failed"));
            }
        }
        let session_directory = match PinnedDirectory::open(&session_path) {
            Ok(directory) => directory,
            Err(error) => {
                let _ = fs::remove_dir(&session_path);
                drop(sessions);
                if created_sessions {
                    let _ = fs::remove_dir(&sessions_path);
                }
                return Err(map_path_error(error));
            }
        };
        let temporary_path = session_path.join(TEMP_SESSION_DOCUMENT);
        let document_path = session_path.join("session.md");

        let result = (|| {
            inject_fault(fault, CreateFault::SessionDirectory)?;
            pinned_project.revalidate().map_err(map_path_error)?;
            sessions.revalidate().map_err(map_path_error)?;
            session_directory.revalidate().map_err(map_path_error)?;

            let document = render_session_document(session)?;
            let mut temporary = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(|_| SessionStoreError::new("session_snapshot_write_failed"))?;
            temporary
                .write_all(document.as_bytes())
                .and_then(|()| temporary.sync_all())
                .map_err(|_| SessionStoreError::new("session_snapshot_write_failed"))?;
            drop(temporary);

            inject_fault(fault, CreateFault::TemporarySynced)?;
            session_directory.revalidate().map_err(map_path_error)?;
            atomic_publish_new(&temporary_path, &document_path)?;
            inject_fault(fault, CreateFault::Published)?;
            pinned_project.revalidate().map_err(map_path_error)?;
            sessions.revalidate().map_err(map_path_error)?;
            session_directory.revalidate().map_err(map_path_error)?;

            Ok(SessionCreateReceipt {
                relative_document: layout.session_document,
                bytes_written: u64::try_from(document.len())
                    .map_err(|_| SessionStoreError::new("session_snapshot_too_large"))?,
            })
        })();

        if result.is_err() {
            // This invocation exclusively created and pinned the session directory, so these
            // fixed-name removals cannot traverse a substituted path.
            let _ = fs::remove_file(&temporary_path);
            let _ = fs::remove_file(&document_path);
        }
        drop(session_directory);
        if result.is_err() {
            let _ = fs::remove_dir(&session_path);
        }
        drop(sessions);
        if result.is_err() && created_sessions {
            let _ = fs::remove_dir(&sessions_path);
        }
        result
    }
}

fn create_directory(path: &Path) -> Result<bool, SessionStoreError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => Err(SessionStoreError::new("session_directory_create_failed")),
    }
}

fn render_session_document(session: &Session) -> Result<String, SessionStoreError> {
    session
        .validate()
        .map_err(|_| SessionStoreError::new("session_contract_invalid"))?;
    let json = serde_json::to_value(session)
        .map_err(|_| SessionStoreError::new("session_snapshot_render_failed"))?;
    let object = json
        .as_object()
        .ok_or_else(|| SessionStoreError::new("session_snapshot_render_failed"))?;
    let scalar = |name: &str| {
        object
            .get(name)
            .ok_or_else(|| SessionStoreError::new("session_snapshot_render_failed"))
            .and_then(render_yaml_flow)
    };

    let mut document = String::from("---\n");
    document.push_str("schema_version: ");
    document.push_str(&scalar("schemaVersion")?);
    document.push_str("\ndocument_type: \"session\"\n");
    for (yaml_name, json_name) in [
        ("id", "id"),
        ("project_id", "projectId"),
        ("folder_name", "folderName"),
        ("title", "title"),
        ("objective", "objective"),
        ("session_context", "sessionContext"),
        ("preset", "preset"),
        ("language", "language"),
        ("microphone", "microphone"),
        ("system_output", "systemOutput"),
        ("transcription_engine", "transcriptionEngine"),
        ("transcription_model_id", "transcriptionModelId"),
        ("llm_models", "llmModels"),
        ("retain_audio", "retainAudio"),
        ("state", "state"),
        ("channel_health", "channelHealth"),
        ("summary_status", "summaryStatus"),
        ("usage", "usage"),
        ("created_at", "createdAt"),
    ] {
        push_yaml_field(&mut document, yaml_name, &scalar(json_name)?);
    }
    for (yaml_name, json_name) in [("started_at", "startedAt"), ("ended_at", "endedAt")] {
        if let Some(value) = object.get(json_name) {
            push_yaml_field(&mut document, yaml_name, &render_yaml_flow(value)?);
        }
    }
    for (yaml_name, json_name) in [("updated_at", "updatedAt"), ("revision", "revision")] {
        push_yaml_field(&mut document, yaml_name, &scalar(json_name)?);
    }
    document.push_str("---\n");
    if document.len() > MAX_SESSION_DOCUMENT_BYTES {
        return Err(SessionStoreError::new("session_snapshot_too_large"));
    }
    Ok(document)
}

fn push_yaml_field(document: &mut String, name: &str, value: &str) {
    document.push_str(name);
    document.push_str(": ");
    document.push_str(value);
    document.push('\n');
}

fn render_yaml_flow(value: &serde_json::Value) -> Result<String, SessionStoreError> {
    match value {
        serde_json::Value::Array(items) => {
            let items = items
                .iter()
                .map(render_yaml_flow)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", items.join(", ")))
        }
        serde_json::Value::Object(fields) if !fields.is_empty() => {
            let fields = fields
                .iter()
                .map(|(name, value)| {
                    let name = serde_json::to_string(name)
                        .map_err(|_| SessionStoreError::new("session_snapshot_render_failed"))?;
                    Ok(format!("{name}: {}", render_yaml_flow(value)?))
                })
                .collect::<Result<Vec<_>, SessionStoreError>>()?;
            Ok(format!("{{{}}}", fields.join(", ")))
        }
        _ => serde_json::to_string(value)
            .map_err(|_| SessionStoreError::new("session_snapshot_render_failed")),
    }
}

fn map_contract_error(code: &'static str) -> SessionStoreError {
    match code {
        "session_project_identity_mismatch" => {
            SessionStoreError::new("session_project_identity_mismatch")
        }
        _ => SessionStoreError::new("session_contract_invalid"),
    }
}

fn map_workspace_error(_error: ProjectStoreError) -> SessionStoreError {
    SessionStoreError::new("session_workspace_invalid")
}

fn map_project_error(error: ProjectStoreError) -> SessionStoreError {
    match error.code {
        "project_contract_invalid" => SessionStoreError::new("session_project_invalid"),
        "project_snapshot_missing" => SessionStoreError::new("session_project_not_found"),
        "project_path_unsafe" | "project_path_identity_changed" => map_path_error(error),
        "project_path_open_failed" => SessionStoreError::new("session_project_not_found"),
        _ => SessionStoreError::new("session_project_unavailable"),
    }
}

fn map_path_error(error: ProjectStoreError) -> SessionStoreError {
    match error.code {
        "project_path_unsafe" => SessionStoreError::new("session_path_unsafe"),
        "project_path_identity_changed" => SessionStoreError::new("session_path_identity_changed"),
        _ => SessionStoreError::new("session_path_open_failed"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateFault {
    SessionDirectory,
    TemporarySynced,
    Published,
}

fn inject_fault(
    configured: Option<CreateFault>,
    current: CreateFault,
) -> Result<(), SessionStoreError> {
    if configured == Some(current) {
        Err(SessionStoreError::new("session_create_fault_injected"))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn atomic_publish_new(source: &Path, destination: &Path) -> Result<(), SessionStoreError> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both paths are NUL-terminated UTF-16 values in the same pinned session
    // directory. Omitting REPLACE_EXISTING ensures a pre-existing snapshot is untouched.
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(SessionStoreError::new("session_snapshot_publish_failed"))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_publish_new(_source: &Path, _destination: &Path) -> Result<(), SessionStoreError> {
    Err(SessionStoreError::new("session_windows_only"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        domain::{CreateSessionSnapshotInput, Project, Session},
        persistence::ProjectStore,
    };

    use super::{CreateFault, SessionStore, render_session_document};

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

    fn initial_session(project: &Project) -> Session {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        let source = &fixture["session"];
        let input: CreateSessionSnapshotInput = serde_json::from_value(serde_json::json!({
            "title": source["title"],
            "objective": source["objective"],
            "sessionContext": source["sessionContext"],
            "preset": source["preset"],
            "language": source["language"],
            "microphone": source["microphone"],
            "systemOutput": source["systemOutput"],
            "transcriptionModelId": source["transcriptionModelId"],
            "llmModels": source["llmModels"],
            "retainAudio": true
        }))
        .unwrap();
        Session::create(project.id, input, "2026-08-11T08:45:00Z".to_owned()).unwrap()
    }

    fn workspace_with_project() -> (tempfile::TempDir, Project, SessionStore) {
        let workspace = tempfile::tempdir().unwrap();
        let (project, _) = records();
        ProjectStore::open(workspace.path())
            .unwrap()
            .create_project(&project)
            .unwrap();
        let store = SessionStore::open(workspace.path()).unwrap();
        (workspace, project, store)
    }

    #[test]
    fn session_markdown_matches_the_golden_snapshot() {
        let (_, session) = records();
        let document = render_session_document(&session).unwrap();

        assert_eq!(
            document,
            include_str!("../../../fixtures/persistence/session-v1.md")
        );
    }

    #[test]
    fn hostile_nested_scalars_cannot_escape_session_front_matter() {
        let (_, session) = records();
        let mut value = serde_json::to_value(session).unwrap();
        value["objective"] = serde_json::json!("Review\n---\ndocument_type: hacked # tag");
        value["preset"]["assistantRole"] =
            serde_json::json!("Do this\n---\n!!python/object payload");
        let session: Session = serde_json::from_value(value).unwrap();

        let document = render_session_document(&session).unwrap();

        assert_eq!(document.lines().filter(|line| *line == "---").count(), 2);
        assert_eq!(document.matches("document_type: \"session\"").count(), 1);
        assert!(document.contains("Review\\n---\\ndocument_type: hacked # tag"));
        assert!(document.contains("Do this\\n---\\n!!python/object payload"));
    }

    #[test]
    fn creation_publishes_only_the_derived_initial_session_snapshot() {
        let (workspace, project, store) = workspace_with_project();
        let session = initial_session(&project);

        let receipt = store.create_session(&project, &session).unwrap();

        assert_eq!(
            receipt.relative_document,
            format!(
                "projects/{}/sessions/{}/session.md",
                project.folder_name, session.folder_name
            )
        );
        let session_directory = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("sessions")
            .join(&session.folder_name);
        let entries = fs::read_dir(&session_directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec!["session.md"]);
        let bytes = fs::read(session_directory.join("session.md")).unwrap();
        assert_eq!(receipt.bytes_written, bytes.len() as u64);
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            render_session_document(&session).unwrap()
        );
        assert!(!session_directory.join("audio").exists());
    }

    #[test]
    fn missing_or_cross_project_parent_is_rejected_before_mutation() {
        let workspace = tempfile::tempdir().unwrap();
        let (project, session) = records();
        let store = SessionStore::open(workspace.path()).unwrap();
        assert_eq!(
            store.create_session(&project, &session).unwrap_err().code,
            "session_project_not_found"
        );
        assert!(!workspace.path().join("projects").exists());

        let (workspace, project, store) = workspace_with_project();
        let mut value = serde_json::to_value(session).unwrap();
        value["projectId"] = serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        let other: Session = serde_json::from_value(value).unwrap();
        assert_eq!(
            store.create_session(&project, &other).unwrap_err().code,
            "session_project_identity_mismatch"
        );
        assert!(
            !workspace
                .path()
                .join("projects")
                .join(project.folder_name)
                .join("sessions")
                .exists()
        );
    }

    #[test]
    fn duplicate_creation_preserves_the_acknowledged_snapshot() {
        let (workspace, project, store) = workspace_with_project();
        let (_, session) = records();
        store.create_session(&project, &session).unwrap();
        let document = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("sessions")
            .join(&session.folder_name)
            .join("session.md");
        let acknowledged = fs::read(&document).unwrap();

        assert_eq!(
            store.create_session(&project, &session).unwrap_err().code,
            "session_already_exists"
        );
        assert_eq!(fs::read(document).unwrap(), acknowledged);
    }

    #[test]
    fn a_reparse_backed_sessions_directory_is_rejected_without_touching_its_target() {
        let (workspace, project, store) = workspace_with_project();
        let (_, session) = records();
        let target = tempfile::tempdir().unwrap();
        let sessions = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("sessions");
        junction::create(target.path(), &sessions).unwrap();

        assert_eq!(
            store.create_session(&project, &session).unwrap_err().code,
            "session_path_unsafe"
        );
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
    }

    #[test]
    fn every_injected_fault_cleans_owned_artifacts_and_preserves_sessions_parent() {
        for fault in [
            CreateFault::SessionDirectory,
            CreateFault::TemporarySynced,
            CreateFault::Published,
        ] {
            let (workspace, project, store) = workspace_with_project();
            let (_, session) = records();
            let sessions = workspace
                .path()
                .join("projects")
                .join(&project.folder_name)
                .join("sessions");
            fs::create_dir(&sessions).unwrap();

            assert_eq!(
                store
                    .create_session_with_fault(&project, &session, Some(fault))
                    .unwrap_err()
                    .code,
                "session_create_fault_injected"
            );
            assert!(sessions.is_dir());
            assert_eq!(fs::read_dir(&sessions).unwrap().count(), 0);
        }
    }

    #[test]
    fn the_store_pins_the_workspace_against_replacement() {
        let (workspace, _project, _store) = workspace_with_project();
        let moved = workspace.path().with_extension("moved");

        assert!(fs::rename(workspace.path(), &moved).is_err());
        assert!(workspace.path().is_dir());
    }
}
