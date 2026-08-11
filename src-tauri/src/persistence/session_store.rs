// P4-006 freezes the Rust-owned session snapshot boundary before a later task adds a
// product session service/command. Keep the implementation compiled and tested now.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::domain::{Project, ProjectId, Session, SessionId};

use super::{
    layout::PortableFolderContract,
    project_store::{
        PinnedDirectory, PinnedProjectDirectory, ProjectLocator, ProjectStore, ProjectStoreError,
        atomic_publish_new, atomic_replace_existing, atomic_replace_with_backup,
        open_snapshot_without_write_share,
    },
};

const TEMP_SESSION_DOCUMENT: &str = ".session.md.tmp";
const BACKUP_SESSION_DOCUMENT: &str = "session.md.bak";
const MAX_SESSION_DOCUMENT_BYTES: u64 = 512 * 1024;
const MAX_SESSION_DISCOVERY_ENTRIES: usize = 4_096;
const MAX_SESSION_DISCOVERY_ISSUES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionStoreError {
    pub(crate) code: &'static str,
}

impl SessionStoreError {
    pub(super) fn new(code: &'static str) -> Self {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionLocator {
    project: ProjectLocator,
    project_id: ProjectId,
    id: SessionId,
    folder_name: String,
}

impl SessionLocator {
    pub(crate) fn from_records(
        project: &Project,
        session: &Session,
    ) -> Result<Self, SessionStoreError> {
        project
            .validate()
            .map_err(|_| SessionStoreError::new("session_project_invalid"))?;
        session
            .validate()
            .map_err(|_| SessionStoreError::new("session_contract_invalid"))?;
        PortableFolderContract::for_records(project, session).map_err(map_contract_error)?;
        Ok(Self {
            project: ProjectLocator::from_project(project).map_err(map_project_error)?,
            project_id: project.id,
            id: session.id,
            folder_name: session.folder_name.clone(),
        })
    }

    pub(super) fn session_id(&self) -> SessionId {
        self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionSnapshotFingerprint([u8; 32]);

impl SessionSnapshotFingerprint {
    pub(super) fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionSnapshot {
    pub(crate) session: Session,
    pub(crate) fingerprint: SessionSnapshotFingerprint,
    pub(crate) recovered_from_backup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionDiscoveryIssue {
    pub(crate) entry_name: Option<String>,
    pub(crate) code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredSession {
    pub(crate) snapshot: SessionSnapshot,
    pub(crate) project_folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionDiscoveryReport {
    pub(crate) sessions: Vec<DiscoveredSession>,
    pub(crate) issues: Vec<SessionDiscoveryIssue>,
    pub(crate) scanned_entries: u32,
    pub(crate) truncated: bool,
    pub(crate) issues_truncated: bool,
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

    pub(crate) fn read_session(
        &self,
        locator: &SessionLocator,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        self.read_session_with_recovery(locator)
    }

    pub(crate) fn update_session(
        &self,
        locator: &SessionLocator,
        updated: &Session,
        expected_revision: u64,
        expected_fingerprint: SessionSnapshotFingerprint,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        self.update_session_with_fault(
            locator,
            updated,
            expected_revision,
            expected_fingerprint,
            None,
        )
    }

    pub(crate) fn discover_sessions(&self) -> Result<SessionDiscoveryReport, SessionStoreError> {
        let projects = self
            .projects
            .discover_projects()
            .map_err(map_project_error)?;
        let mut report = SessionDiscoveryReport {
            sessions: Vec::new(),
            issues: Vec::new(),
            scanned_entries: 0,
            truncated: projects.truncated,
            issues_truncated: projects.issues_truncated,
        };
        for issue in projects.issues {
            push_discovery_issue(&mut report, issue.entry_name, issue.code);
        }
        if report.truncated {
            return Ok(report);
        }

        let mut candidates = Vec::new();
        for project_snapshot in projects.projects {
            let project = project_snapshot.project;
            let project_locator =
                ProjectLocator::from_project(&project).map_err(map_project_error)?;
            let pinned_project = self
                .projects
                .open_existing_project(&project_locator)
                .map_err(map_project_error)?;
            let sessions_path = pinned_project.path().join("sessions");
            if !sessions_path.exists() {
                continue;
            }
            let sessions_directory =
                PinnedDirectory::open(&sessions_path).map_err(map_path_error)?;
            pinned_project.revalidate().map_err(map_path_error)?;
            let mut entries = fs::read_dir(&sessions_path)
                .map_err(|_| SessionStoreError::new("session_discovery_failed"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| SessionStoreError::new("session_discovery_failed"))?;
            report.scanned_entries = report
                .scanned_entries
                .saturating_add(u32::try_from(entries.len()).unwrap_or(u32::MAX));
            if usize::try_from(report.scanned_entries).unwrap_or(usize::MAX)
                > MAX_SESSION_DISCOVERY_ENTRIES
            {
                report.sessions.clear();
                report.truncated = true;
                push_discovery_issue(&mut report, None, "session_discovery_limit_exceeded");
                return Ok(report);
            }
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let file_name = entry.file_name();
                let entry_name = safe_discovery_entry_name(&project.folder_name, &file_name);
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => {
                        push_discovery_issue(
                            &mut report,
                            entry_name,
                            "session_entry_metadata_unavailable",
                        );
                        continue;
                    }
                };
                if !file_type.is_dir() || file_type.is_symlink() {
                    push_discovery_issue(&mut report, entry_name, "session_entry_not_directory");
                    continue;
                }
                let Some(folder_name) = file_name.to_str() else {
                    push_discovery_issue(&mut report, None, "session_entry_name_invalid");
                    continue;
                };
                if folder_name.is_empty()
                    || folder_name.len() > 128
                    || folder_name.chars().any(char::is_control)
                {
                    push_discovery_issue(&mut report, entry_name, "session_entry_name_invalid");
                    continue;
                }
                match self.read_discovered_session(&project, folder_name) {
                    Ok(snapshot) => candidates.push((
                        DiscoveredSession {
                            snapshot,
                            project_folder: project.folder_name.clone(),
                        },
                        entry_name,
                    )),
                    Err(error) => push_discovery_issue(&mut report, entry_name, error.code),
                }
            }
            sessions_directory.revalidate().map_err(map_path_error)?;
            pinned_project.revalidate().map_err(map_path_error)?;
        }

        let mut id_counts = HashMap::new();
        for (snapshot, _) in &candidates {
            *id_counts
                .entry(snapshot.snapshot.session.id)
                .or_insert(0_u32) += 1;
        }
        for (snapshot, entry_name) in candidates {
            if id_counts.get(&snapshot.snapshot.session.id) == Some(&1) {
                report.sessions.push(snapshot);
            } else {
                push_discovery_issue(&mut report, entry_name, "session_duplicate_id");
            }
        }
        report.sessions.sort_by(|left, right| {
            let meeting_time = |session: &Session| {
                OffsetDateTime::parse(
                    session.started_at.as_deref().unwrap_or(&session.created_at),
                    &Rfc3339,
                )
                .expect("validated session timestamp")
            };
            let updated = |session: &Session| {
                OffsetDateTime::parse(&session.updated_at, &Rfc3339)
                    .expect("validated session timestamp")
            };
            meeting_time(&right.snapshot.session)
                .cmp(&meeting_time(&left.snapshot.session))
                .then_with(|| {
                    updated(&right.snapshot.session).cmp(&updated(&left.snapshot.session))
                })
                .then_with(|| left.project_folder.cmp(&right.project_folder))
                .then_with(|| {
                    left.snapshot
                        .session
                        .folder_name
                        .cmp(&right.snapshot.session.folder_name)
                })
        });
        Ok(report)
    }

    pub(crate) fn read_session_by_id(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        self.discover_sessions()?
            .sessions
            .into_iter()
            .find(|candidate| {
                candidate.snapshot.session.project_id == project_id
                    && candidate.snapshot.session.id == session_id
            })
            .map(|candidate| candidate.snapshot)
            .ok_or_else(|| SessionStoreError::new("session_not_found"))
    }

    fn read_discovered_session(
        &self,
        project: &Project,
        folder_name: &str,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        let project_locator = ProjectLocator::from_project(project).map_err(map_project_error)?;
        let pinned_project = self
            .projects
            .open_existing_project(&project_locator)
            .map_err(map_project_error)?;
        let sessions_path = pinned_project.path().join("sessions");
        let sessions = PinnedDirectory::open(&sessions_path).map_err(map_path_error)?;
        let session_path = sessions_path.join(folder_name);
        let session_directory = PinnedDirectory::open(&session_path).map_err(map_path_error)?;
        pinned_project.revalidate().map_err(map_path_error)?;
        sessions.revalidate().map_err(map_path_error)?;
        session_directory.revalidate().map_err(map_path_error)?;
        let paths = SessionDocumentPaths::new(&session_path);
        let candidate = match read_locked_snapshot_internal(&paths.document, None) {
            Ok(snapshot) => snapshot,
            Err(error) if error.error.is_recoverable_snapshot_failure() => {
                read_locked_snapshot_internal(&paths.backup, None)
                    .map_err(|_| SessionStoreError::new("session_snapshot_recovery_failed"))?
            }
            Err(error) => return Err(error.error),
        };
        if candidate.session.project_id != project.id
            || candidate.session.folder_name != folder_name
        {
            return Err(SessionStoreError::new("session_snapshot_identity_mismatch"));
        }
        let locator = SessionLocator::from_records(project, &candidate.session)?;
        drop(candidate);
        drop(session_directory);
        drop(sessions);
        drop(pinned_project);
        self.read_session(&locator)
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
            atomic_publish_new(&temporary_path, &document_path)
                .map_err(map_snapshot_publish_error)?;
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

    fn read_session_with_recovery(
        &self,
        locator: &SessionLocator,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        let pinned = self.open_existing_session(locator)?;
        let paths = SessionDocumentPaths::new(pinned.path());
        match read_locked_snapshot(&paths.document, locator) {
            Ok(snapshot) => {
                let _ = fs::remove_file(&paths.temporary);
                Ok(snapshot.into_public(false))
            }
            Err(error) if error.is_recoverable_snapshot_failure() => {
                let main = match read_locked_snapshot_internal(&paths.document, Some(locator)) {
                    Ok(snapshot) => return Ok(snapshot.into_public(false)),
                    Err(error) => error,
                };
                if !main.error.is_recoverable_snapshot_failure() {
                    return Err(main.error);
                }
                let missing = main.error.code == "session_snapshot_missing";
                let backup = read_locked_snapshot(&paths.backup, locator)
                    .map_err(|_| SessionStoreError::new("session_snapshot_recovery_failed"))?;
                pinned.revalidate()?;
                publish_recovery_bytes(&paths, &backup.bytes, !missing)?;
                drop(main);
                let restored = read_locked_snapshot(&paths.document, locator)
                    .map_err(|_| SessionStoreError::new("session_snapshot_recovery_failed"))?;
                Ok(restored.into_public(true))
            }
            Err(error) => Err(error),
        }
    }

    fn update_session_with_fault(
        &self,
        locator: &SessionLocator,
        updated: &Session,
        expected_revision: u64,
        expected_fingerprint: SessionSnapshotFingerprint,
        fault: Option<UpdateFault>,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        updated
            .validate()
            .map_err(|_| SessionStoreError::new("session_contract_invalid"))?;
        let pinned = self.open_existing_session(locator)?;
        let paths = SessionDocumentPaths::new(pinned.path());
        let current = read_locked_snapshot(&paths.document, locator)?;

        validate_session_update(
            &current.session,
            updated,
            expected_revision,
            expected_fingerprint,
            current.fingerprint,
        )?;
        let document = render_session_document(updated)?;
        write_synced_temporary(&paths.temporary, document.as_bytes())?;

        let mut replaced = false;
        let result = (|| {
            inject_update_fault(fault, UpdateFault::TemporarySynced)?;
            pinned.revalidate()?;
            remove_if_file(&paths.backup)?;
            atomic_replace_with_backup(&paths.temporary, &paths.document, &paths.backup)
                .map_err(map_snapshot_publish_error)?;
            replaced = true;
            inject_update_fault(fault, UpdateFault::Replaced)?;
            pinned.revalidate()?;
            let snapshot = read_locked_snapshot(&paths.document, locator)?;
            if snapshot.session != *updated {
                return Err(SessionStoreError::new("session_snapshot_verify_failed"));
            }
            Ok(snapshot.into_public(false))
        })();

        drop(current);
        if result.is_err() {
            let _ = fs::remove_file(&paths.temporary);
            if replaced {
                let _ = restore_backup(&paths, locator);
            }
        }
        result
    }

    pub(super) fn open_existing_session(
        &self,
        locator: &SessionLocator,
    ) -> Result<PinnedSessionDirectory, SessionStoreError> {
        let project = self
            .projects
            .open_existing_project(&locator.project)
            .map_err(map_project_error)?;
        let authoritative = self
            .projects
            .read_project(&locator.project)
            .map_err(map_project_error)?;
        if authoritative.project.id != locator.project_id {
            return Err(SessionStoreError::new("session_project_identity_mismatch"));
        }
        let sessions_path = project.path().join("sessions");
        if !sessions_path.exists() {
            return Err(SessionStoreError::new("session_snapshot_missing"));
        }
        let sessions = PinnedDirectory::open(&sessions_path).map_err(map_path_error)?;
        let session_path = sessions_path.join(&locator.folder_name);
        if !session_path.exists() {
            return Err(SessionStoreError::new("session_snapshot_missing"));
        }
        let session = PinnedDirectory::open(&session_path).map_err(map_path_error)?;
        project.revalidate().map_err(map_path_error)?;
        sessions.revalidate().map_err(map_path_error)?;
        session.revalidate().map_err(map_path_error)?;
        Ok(PinnedSessionDirectory {
            project,
            sessions,
            session,
        })
    }
}

pub(super) struct PinnedSessionDirectory {
    project: PinnedProjectDirectory,
    sessions: PinnedDirectory,
    session: PinnedDirectory,
}

impl PinnedSessionDirectory {
    pub(super) fn path(&self) -> &Path {
        self.session.path()
    }

    pub(super) fn revalidate(&self) -> Result<(), SessionStoreError> {
        self.project.revalidate().map_err(map_path_error)?;
        self.sessions.revalidate().map_err(map_path_error)?;
        self.session.revalidate().map_err(map_path_error)
    }
}

struct SessionDocumentPaths {
    document: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
}

impl SessionDocumentPaths {
    fn new(session_directory: &Path) -> Self {
        Self {
            document: session_directory.join("session.md"),
            temporary: session_directory.join(TEMP_SESSION_DOCUMENT),
            backup: session_directory.join(BACKUP_SESSION_DOCUMENT),
        }
    }
}

struct LockedSessionSnapshot {
    _file: File,
    bytes: Vec<u8>,
    session: Session,
    fingerprint: SessionSnapshotFingerprint,
}

#[derive(Debug)]
struct LockedSnapshotReadError {
    error: SessionStoreError,
    _file: Option<File>,
}

impl LockedSessionSnapshot {
    fn into_public(self, recovered_from_backup: bool) -> SessionSnapshot {
        SessionSnapshot {
            session: self.session,
            fingerprint: self.fingerprint,
            recovered_from_backup,
        }
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
    if u64::try_from(document.len())
        .map_err(|_| SessionStoreError::new("session_snapshot_too_large"))?
        > MAX_SESSION_DOCUMENT_BYTES
    {
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

fn read_locked_snapshot(
    path: &Path,
    locator: &SessionLocator,
) -> Result<LockedSessionSnapshot, SessionStoreError> {
    read_locked_snapshot_internal(path, Some(locator)).map_err(|error| error.error)
}

fn safe_discovery_entry_name(project_folder: &str, name: &std::ffi::OsStr) -> Option<String> {
    let name = name.to_str()?;
    if name.is_empty()
        || name.len() > 128
        || name.chars().any(char::is_control)
        || project_folder.chars().any(char::is_control)
    {
        return None;
    }
    let combined = format!("{project_folder}/{name}");
    (combined.len() <= 257).then_some(combined)
}

fn push_discovery_issue(
    report: &mut SessionDiscoveryReport,
    entry_name: Option<String>,
    code: &'static str,
) {
    if report.issues.len() < MAX_SESSION_DISCOVERY_ISSUES {
        report
            .issues
            .push(SessionDiscoveryIssue { entry_name, code });
    } else {
        report.issues_truncated = true;
    }
}

fn read_locked_snapshot_internal(
    path: &Path,
    locator: Option<&SessionLocator>,
) -> Result<LockedSessionSnapshot, LockedSnapshotReadError> {
    let mut file = open_snapshot_without_write_share(path)
        .map_err(map_snapshot_open_error)
        .map_err(|error| LockedSnapshotReadError { error, _file: None })?;
    let length = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => {
            return Err(LockedSnapshotReadError {
                error: SessionStoreError::new("session_snapshot_read_failed"),
                _file: Some(file),
            });
        }
    };
    if length > MAX_SESSION_DOCUMENT_BYTES {
        return Err(LockedSnapshotReadError {
            error: SessionStoreError::new("session_snapshot_too_large"),
            _file: Some(file),
        });
    }
    let capacity = match usize::try_from(length) {
        Ok(capacity) => capacity,
        Err(_) => {
            return Err(LockedSnapshotReadError {
                error: SessionStoreError::new("session_snapshot_too_large"),
                _file: Some(file),
            });
        }
    };
    let mut bytes = Vec::with_capacity(capacity);
    if Read::by_ref(&mut file)
        .take(MAX_SESSION_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(LockedSnapshotReadError {
            error: SessionStoreError::new("session_snapshot_read_failed"),
            _file: Some(file),
        });
    }
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAX_SESSION_DOCUMENT_BYTES) {
        return Err(LockedSnapshotReadError {
            error: SessionStoreError::new("session_snapshot_too_large"),
            _file: Some(file),
        });
    }
    let session = match parse_session_document(&bytes) {
        Ok(session) => session,
        Err(error) => {
            return Err(LockedSnapshotReadError {
                error,
                _file: Some(file),
            });
        }
    };
    if locator.is_some_and(|locator| {
        session.id != locator.id
            || session.project_id != locator.project_id
            || session.folder_name != locator.folder_name
    }) {
        return Err(LockedSnapshotReadError {
            error: SessionStoreError::new("session_snapshot_identity_mismatch"),
            _file: Some(file),
        });
    }
    let fingerprint = SessionSnapshotFingerprint(Sha256::digest(&bytes).into());
    Ok(LockedSessionSnapshot {
        _file: file,
        bytes,
        session,
        fingerprint,
    })
}

fn parse_session_document(bytes: &[u8]) -> Result<Session, SessionStoreError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| SessionStoreError::new("session_snapshot_invalid"))?;
    let normalized = text.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(SessionStoreError::new("session_snapshot_invalid"));
    }
    let mut lines = normalized.lines().peekable();
    if lines.next() != Some("---") {
        return Err(SessionStoreError::new("session_snapshot_invalid"));
    }

    let mut object = serde_json::Map::new();
    for (yaml_name, json_name) in [
        ("schema_version", "schemaVersion"),
        ("document_type", "documentType"),
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
        parse_exact_field(&mut lines, &mut object, yaml_name, json_name)?;
    }
    for (yaml_name, json_name) in [("started_at", "startedAt"), ("ended_at", "endedAt")] {
        if lines
            .peek()
            .is_some_and(|line| line.starts_with(&format!("{yaml_name}: ")))
        {
            parse_exact_field(&mut lines, &mut object, yaml_name, json_name)?;
        }
    }
    for (yaml_name, json_name) in [("updated_at", "updatedAt"), ("revision", "revision")] {
        parse_exact_field(&mut lines, &mut object, yaml_name, json_name)?;
    }
    if lines.next() != Some("---") || lines.next().is_some() {
        return Err(SessionStoreError::new("session_snapshot_invalid"));
    }
    if object.remove("documentType") != Some(serde_json::json!("session")) {
        return Err(SessionStoreError::new("session_snapshot_invalid"));
    }
    serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|_| SessionStoreError::new("session_snapshot_invalid"))
}

fn parse_exact_field<'a, I>(
    lines: &mut std::iter::Peekable<I>,
    object: &mut serde_json::Map<String, serde_json::Value>,
    yaml_name: &str,
    json_name: &str,
) -> Result<(), SessionStoreError>
where
    I: Iterator<Item = &'a str>,
{
    let line = lines
        .next()
        .ok_or_else(|| SessionStoreError::new("session_snapshot_invalid"))?;
    let (name, encoded) = line
        .split_once(": ")
        .ok_or_else(|| SessionStoreError::new("session_snapshot_invalid"))?;
    if name != yaml_name {
        return Err(SessionStoreError::new("session_snapshot_invalid"));
    }
    let value = serde_json::from_str(encoded)
        .map_err(|_| SessionStoreError::new("session_snapshot_invalid"))?;
    object.insert(json_name.to_owned(), value);
    Ok(())
}

fn validate_session_update(
    current: &Session,
    updated: &Session,
    expected_revision: u64,
    expected_fingerprint: SessionSnapshotFingerprint,
    current_fingerprint: SessionSnapshotFingerprint,
) -> Result<(), SessionStoreError> {
    if current.revision != expected_revision {
        return Err(SessionStoreError::new("session_revision_conflict"));
    }
    if expected_fingerprint != current_fingerprint {
        return Err(SessionStoreError::new("session_external_modification"));
    }
    if current.id != updated.id
        || current.project_id != updated.project_id
        || current.folder_name != updated.folder_name
        || current.created_at != updated.created_at
    {
        return Err(SessionStoreError::new("session_snapshot_identity_mismatch"));
    }
    if expected_revision.checked_add(1) != Some(updated.revision) {
        return Err(SessionStoreError::new("session_revision_invalid"));
    }
    let current_updated = OffsetDateTime::parse(&current.updated_at, &Rfc3339)
        .map_err(|_| SessionStoreError::new("session_snapshot_invalid"))?;
    let next_updated = OffsetDateTime::parse(&updated.updated_at, &Rfc3339)
        .map_err(|_| SessionStoreError::new("session_contract_invalid"))?;
    if next_updated < current_updated {
        return Err(SessionStoreError::new("session_revision_invalid"));
    }
    Ok(())
}

fn write_synced_temporary(path: &Path, bytes: &[u8]) -> Result<(), SessionStoreError> {
    remove_if_file(path)?;
    let mut temporary = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| SessionStoreError::new("session_snapshot_write_failed"))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.sync_all())
        .map_err(|_| SessionStoreError::new("session_snapshot_write_failed"))
}

fn publish_recovery_bytes(
    paths: &SessionDocumentPaths,
    bytes: &[u8],
    replace_existing: bool,
) -> Result<(), SessionStoreError> {
    write_synced_temporary(&paths.temporary, bytes)?;
    let result = if replace_existing {
        atomic_replace_existing(&paths.temporary, &paths.document)
    } else {
        atomic_publish_new(&paths.temporary, &paths.document)
    };
    result
        .map_err(map_snapshot_publish_error)
        .map_err(|_| SessionStoreError::new("session_snapshot_recovery_failed"))
}

fn restore_backup(
    paths: &SessionDocumentPaths,
    locator: &SessionLocator,
) -> Result<(), SessionStoreError> {
    let backup = read_locked_snapshot(&paths.backup, locator)?;
    publish_recovery_bytes(paths, &backup.bytes, true)
}

fn remove_if_file(path: &Path) -> Result<(), SessionStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SessionStoreError::new("session_snapshot_cleanup_failed")),
    }
}

impl SessionStoreError {
    fn is_recoverable_snapshot_failure(self) -> bool {
        matches!(
            self.code,
            "session_snapshot_missing" | "session_snapshot_invalid" | "session_snapshot_too_large"
        )
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

fn map_snapshot_open_error(error: ProjectStoreError) -> SessionStoreError {
    match error.code {
        "project_snapshot_missing" => SessionStoreError::new("session_snapshot_missing"),
        "project_path_unsafe" => SessionStoreError::new("session_path_unsafe"),
        "project_path_identity_unavailable" => {
            SessionStoreError::new("session_path_identity_unavailable")
        }
        _ => SessionStoreError::new("session_snapshot_read_failed"),
    }
}

fn map_snapshot_publish_error(_error: ProjectStoreError) -> SessionStoreError {
    SessionStoreError::new("session_snapshot_publish_failed")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateFault {
    TemporarySynced,
    Replaced,
}

fn inject_update_fault(
    configured: Option<UpdateFault>,
    current: UpdateFault,
) -> Result<(), SessionStoreError> {
    if configured == Some(current) {
        Err(SessionStoreError::new("session_update_fault_injected"))
    } else {
        Ok(())
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use crate::{
        domain::{CreateSessionSnapshotInput, Project, Session},
        persistence::ProjectStore,
    };

    use super::{
        CreateFault, MAX_SESSION_DISCOVERY_ENTRIES, MAX_SESSION_DOCUMENT_BYTES, SessionLocator,
        SessionStore, UpdateFault, parse_session_document, render_session_document,
    };

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

    fn updated_session() -> Session {
        let (_, session) = records();
        let mut value = serde_json::to_value(session).unwrap();
        value["title"] = serde_json::json!("Sprint planning updated");
        value["updatedAt"] = serde_json::json!("2026-08-11T09:05:00Z");
        value["revision"] = serde_json::json!(4);
        serde_json::from_value(value).unwrap()
    }

    fn workspace_with_session() -> (tempfile::TempDir, Project, Session, SessionStore) {
        let (workspace, project, store) = workspace_with_project();
        let (_, session) = records();
        store.create_session(&project, &session).unwrap();
        (workspace, project, session, store)
    }

    fn session_paths(
        workspace: &Path,
        project: &Project,
        session: &Session,
    ) -> (PathBuf, PathBuf, PathBuf) {
        let directory = workspace
            .join("projects")
            .join(&project.folder_name)
            .join("sessions")
            .join(&session.folder_name);
        (
            directory.join("session.md"),
            directory.join("session.md.bak"),
            directory.join(".session.md.tmp"),
        )
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
    fn strict_session_parser_accepts_only_the_canonical_lf_or_crlf_shape() {
        let (_, session) = records();
        let document = render_session_document(&session).unwrap();
        assert_eq!(
            parse_session_document(document.as_bytes()).unwrap(),
            session
        );
        let crlf = document.replace('\n', "\r\n");
        assert_eq!(parse_session_document(crlf.as_bytes()).unwrap(), session);

        let invalid = [
            document.replace("document_type: \"session\"", "document_type: \"project\""),
            document.replace("title: ", "unexpected_title: "),
            document.replace("started_at: \"2026-08-11T09:00:00Z\"", "started_at: null"),
            format!("{document}trailing"),
            document.replace("\nrevision: 3\n", "\nrevision: 3\nunknown: true\n"),
        ];
        for value in invalid {
            assert_eq!(
                parse_session_document(value.as_bytes()).unwrap_err().code,
                "session_snapshot_invalid"
            );
        }
    }

    #[test]
    fn update_requires_revision_and_fingerprint_and_keeps_a_valid_backup() {
        let (workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let initial = store.read_session(&locator).unwrap();

        let updated = store
            .update_session(
                &locator,
                &updated_session(),
                initial.session.revision,
                initial.fingerprint,
            )
            .unwrap();
        let (document, backup, temporary) = session_paths(workspace.path(), &project, &session);

        assert_eq!(updated.session, updated_session());
        assert!(!updated.recovered_from_backup);
        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            render_session_document(&session).unwrap()
        );
        assert_eq!(
            fs::read_to_string(document).unwrap(),
            render_session_document(&updated_session()).unwrap()
        );
        assert!(!temporary.exists());
    }

    #[test]
    fn external_edits_and_revision_conflicts_never_get_overwritten() {
        let (workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let initial = store.read_session(&locator).unwrap();
        let (document, _, _) = session_paths(workspace.path(), &project, &session);

        let mut external_value = serde_json::to_value(&session).unwrap();
        external_value["objective"] = serde_json::json!("External edit at the same revision");
        let external: Session = serde_json::from_value(external_value).unwrap();
        let external_bytes = render_session_document(&external).unwrap();
        fs::write(&document, &external_bytes).unwrap();

        assert_eq!(
            store
                .update_session(
                    &locator,
                    &updated_session(),
                    session.revision,
                    initial.fingerprint,
                )
                .unwrap_err()
                .code,
            "session_external_modification"
        );
        assert_eq!(fs::read_to_string(&document).unwrap(), external_bytes);

        let external_snapshot = store.read_session(&locator).unwrap();
        assert_eq!(
            store
                .update_session(
                    &locator,
                    &updated_session(),
                    session.revision - 1,
                    external_snapshot.fingerprint,
                )
                .unwrap_err()
                .code,
            "session_revision_conflict"
        );
        assert_eq!(fs::read_to_string(document).unwrap(), external_bytes);
    }

    #[test]
    fn immutable_identity_revision_progression_and_time_order_are_enforced() {
        let (_workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let initial = store.read_session(&locator).unwrap();

        let mut identity_value = serde_json::to_value(updated_session()).unwrap();
        identity_value["createdAt"] = serde_json::json!("2026-08-11T08:44:00Z");
        let identity_change: Session = serde_json::from_value(identity_value).unwrap();
        assert_eq!(
            store
                .update_session(
                    &locator,
                    &identity_change,
                    session.revision,
                    initial.fingerprint,
                )
                .unwrap_err()
                .code,
            "session_snapshot_identity_mismatch"
        );

        let mut revision_value = serde_json::to_value(updated_session()).unwrap();
        revision_value["revision"] = serde_json::json!(5);
        let skipped_revision: Session = serde_json::from_value(revision_value).unwrap();
        assert_eq!(
            store
                .update_session(
                    &locator,
                    &skipped_revision,
                    session.revision,
                    initial.fingerprint,
                )
                .unwrap_err()
                .code,
            "session_revision_invalid"
        );

        let mut time_value = serde_json::to_value(updated_session()).unwrap();
        time_value["updatedAt"] = serde_json::json!("2026-08-11T09:00:01Z");
        let backwards_time: Session = serde_json::from_value(time_value).unwrap();
        assert_eq!(
            store
                .update_session(
                    &locator,
                    &backwards_time,
                    session.revision,
                    initial.fingerprint,
                )
                .unwrap_err()
                .code,
            "session_revision_invalid"
        );
    }

    #[test]
    fn valid_backup_recovers_missing_malformed_and_oversized_documents() {
        for failure in ["missing", "malformed", "oversized"] {
            let (workspace, project, session, store) = workspace_with_session();
            let locator = SessionLocator::from_records(&project, &session).unwrap();
            let initial = store.read_session(&locator).unwrap();
            store
                .update_session(
                    &locator,
                    &updated_session(),
                    session.revision,
                    initial.fingerprint,
                )
                .unwrap();
            let (document, backup, _) = session_paths(workspace.path(), &project, &session);
            let backup_bytes = fs::read(&backup).unwrap();

            match failure {
                "missing" => fs::remove_file(&document).unwrap(),
                "malformed" => fs::write(&document, b"---\ntorn").unwrap(),
                "oversized" => fs::write(
                    &document,
                    vec![b'x'; usize::try_from(MAX_SESSION_DOCUMENT_BYTES + 1).unwrap()],
                )
                .unwrap(),
                _ => unreachable!(),
            }

            let recovered = store
                .read_session(&locator)
                .unwrap_or_else(|error| panic!("{failure}: {error:?}"));
            assert!(recovered.recovered_from_backup, "{failure}");
            assert_eq!(recovered.session, session, "{failure}");
            assert_eq!(fs::read(&document).unwrap(), backup_bytes, "{failure}");
            assert_eq!(fs::read(&backup).unwrap(), backup_bytes, "{failure}");
        }
    }

    #[test]
    fn unrecoverable_input_is_preserved_and_torn_temp_is_cleaned_only_when_main_is_valid() {
        let (workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let (document, _, temporary) = session_paths(workspace.path(), &project, &session);

        fs::write(&temporary, b"torn temp").unwrap();
        store.read_session(&locator).unwrap();
        assert!(!temporary.exists());

        let malformed = b"---\ntorn";
        fs::write(&document, malformed).unwrap();
        assert_eq!(
            store.read_session(&locator).unwrap_err().code,
            "session_snapshot_recovery_failed"
        );
        assert_eq!(fs::read(document).unwrap(), malformed);
    }

    #[test]
    fn update_faults_leave_the_last_acknowledged_snapshot_readable() {
        for fault in [UpdateFault::TemporarySynced, UpdateFault::Replaced] {
            let (workspace, project, session, store) = workspace_with_session();
            let locator = SessionLocator::from_records(&project, &session).unwrap();
            let initial = store.read_session(&locator).unwrap();

            assert_eq!(
                store
                    .update_session_with_fault(
                        &locator,
                        &updated_session(),
                        session.revision,
                        initial.fingerprint,
                        Some(fault),
                    )
                    .unwrap_err()
                    .code,
                "session_update_fault_injected"
            );
            let current = store.read_session(&locator).unwrap();
            assert_eq!(current.session, session, "{fault:?}");
            assert!(
                !session_paths(workspace.path(), &project, &session)
                    .2
                    .exists(),
                "{fault:?}"
            );
        }
    }

    #[test]
    fn session_identity_mismatch_and_reparse_substitution_fail_closed() {
        let (workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let (document, _, _) = session_paths(workspace.path(), &project, &session);
        let mut other_value = serde_json::to_value(&session).unwrap();
        other_value["id"] = serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        other_value["folderName"] = serde_json::json!("2026-08-11-sprint-planning--bbbbbbbb");
        let other: Session = serde_json::from_value(other_value).unwrap();
        fs::write(&document, render_session_document(&other).unwrap()).unwrap();
        assert_eq!(
            store.read_session(&locator).unwrap_err().code,
            "session_snapshot_identity_mismatch"
        );

        let (workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let session_directory = session_paths(workspace.path(), &project, &session)
            .0
            .parent()
            .unwrap()
            .to_owned();
        fs::remove_file(session_directory.join("session.md")).unwrap();
        fs::remove_dir(&session_directory).unwrap();
        let target = tempfile::tempdir().unwrap();
        junction::create(target.path(), &session_directory).unwrap();

        assert_eq!(
            store.read_session(&locator).unwrap_err().code,
            "session_path_unsafe"
        );
        assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_store_pins_the_workspace_against_replacement() {
        let (workspace, _project, _store) = workspace_with_project();
        let moved = workspace.path().with_extension("moved");

        assert!(fs::rename(workspace.path(), &moved).is_err());
        assert!(workspace.path().is_dir());
    }

    #[test]
    fn discovery_reads_only_direct_children_and_reports_invalid_entries() {
        let (workspace, project, session, store) = workspace_with_session();
        let sessions = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("sessions");
        fs::write(sessions.join("unexpected.txt"), b"ignored").unwrap();
        let nested = sessions.join("container").join(&session.folder_name);
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("session.md"),
            render_session_document(&session).unwrap(),
        )
        .unwrap();

        let report = store.discover_sessions().unwrap();

        assert_eq!(report.scanned_entries, 3);
        assert_eq!(report.sessions.len(), 1);
        assert_eq!(report.sessions[0].snapshot.session, session);
        assert_eq!(report.sessions[0].project_folder, project.folder_name);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "session_entry_not_directory"
                && issue
                    .entry_name
                    .as_deref()
                    .is_some_and(|name| name.ends_with("/unexpected.txt"))
        }));
        assert!(report.issues.iter().any(|issue| {
            issue
                .entry_name
                .as_deref()
                .is_some_and(|name| name.ends_with("/container"))
        }));
    }

    #[test]
    fn discovery_recovers_a_valid_backup_and_marks_the_snapshot() {
        let (workspace, project, session, store) = workspace_with_session();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let initial = store.read_session(&locator).unwrap();
        store
            .update_session(
                &locator,
                &updated_session(),
                session.revision,
                initial.fingerprint,
            )
            .unwrap();
        let (document, _, _) = session_paths(workspace.path(), &project, &session);
        fs::write(document, b"---\ntorn").unwrap();

        let report = store.discover_sessions().unwrap();

        assert_eq!(report.sessions.len(), 1);
        assert_eq!(report.sessions[0].snapshot.session, session);
        assert!(report.sessions[0].snapshot.recovered_from_backup);
    }

    #[test]
    fn discovery_bounds_issue_detail_without_hiding_that_it_was_truncated() {
        let (workspace, project, store) = workspace_with_project();
        let sessions = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("sessions");
        fs::create_dir(&sessions).unwrap();
        for index in 0..300 {
            fs::write(sessions.join(format!("unexpected-{index}")), b"ignored").unwrap();
        }

        let report = store.discover_sessions().unwrap();

        assert_eq!(report.issues.len(), 256);
        assert!(report.issues_truncated);
        assert!(!report.truncated);
    }

    #[test]
    fn discovery_excludes_every_candidate_with_a_duplicate_global_session_id() {
        let workspace = tempfile::tempdir().unwrap();
        let (first_project, first_session) = records();
        let mut project_value = serde_json::to_value(&first_project).unwrap();
        project_value["id"] = serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        project_value["name"] = serde_json::json!("Second Project");
        project_value["folderName"] = serde_json::json!("second-project--bbbbbbbb");
        let second_project: Project = serde_json::from_value(project_value).unwrap();
        let mut session_value = serde_json::to_value(&first_session).unwrap();
        session_value["projectId"] = serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        let second_session: Session = serde_json::from_value(session_value).unwrap();
        let projects = ProjectStore::open(workspace.path()).unwrap();
        projects.create_project(&first_project).unwrap();
        projects.create_project(&second_project).unwrap();
        let store = SessionStore::open(workspace.path()).unwrap();
        store
            .create_session(&first_project, &first_session)
            .unwrap();
        store
            .create_session(&second_project, &second_session)
            .unwrap();

        let report = store.discover_sessions().unwrap();

        assert!(report.sessions.is_empty());
        assert_eq!(
            report
                .issues
                .iter()
                .filter(|issue| issue.code == "session_duplicate_id")
                .count(),
            2
        );
    }

    #[test]
    fn discovery_limit_fails_closed_without_returning_a_partial_catalog() {
        let (workspace, project, store) = workspace_with_project();
        let sessions = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("sessions");
        fs::create_dir(&sessions).unwrap();
        for index in 0..=MAX_SESSION_DISCOVERY_ENTRIES {
            fs::create_dir(sessions.join(format!("candidate-{index}"))).unwrap();
        }

        let report = store.discover_sessions().unwrap();

        assert!(report.sessions.is_empty());
        assert!(report.truncated);
        assert_eq!(report.scanned_entries, 4_097);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "session_discovery_limit_exceeded")
        );
    }
}
