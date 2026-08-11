use std::{
    collections::HashMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::os::windows::{
    ffi::OsStrExt,
    fs::OpenOptionsExt,
    io::{AsRawHandle, RawHandle},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetFileInformationByHandle, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
};

use crate::{
    domain::{Project, ProjectId},
    security::{probe_workspace, reject_reparse_points},
};

use super::layout::PortableProjectLayout;

const TEMP_PROJECT_DOCUMENT: &str = ".project.md.tmp";
const BACKUP_PROJECT_DOCUMENT: &str = "project.md.bak";
const MAX_PROJECT_DOCUMENT_BYTES: u64 = 128 * 1024;
const MAX_DISCOVERY_ENTRIES: usize = 4_096;
const MAX_DISCOVERY_ISSUES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectStoreError {
    pub(crate) code: &'static str,
}

impl ProjectStoreError {
    pub(super) fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for ProjectStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for ProjectStoreError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectCreateReceipt {
    pub(crate) relative_document: String,
    pub(crate) bytes_written: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectLocator {
    id: ProjectId,
    folder_name: String,
}

impl ProjectLocator {
    pub(crate) fn from_project(project: &Project) -> Result<Self, ProjectStoreError> {
        project
            .validate()
            .map_err(|_| ProjectStoreError::new("project_contract_invalid"))?;
        Ok(Self {
            id: project.id,
            folder_name: project.folder_name.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectSnapshotFingerprint([u8; 32]);

impl ProjectSnapshotFingerprint {
    pub(super) fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectSnapshot {
    pub(crate) project: Project,
    pub(crate) fingerprint: ProjectSnapshotFingerprint,
    pub(crate) recovered_from_backup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDiscoveryIssue {
    pub(crate) entry_name: Option<String>,
    pub(crate) code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDiscoveryReport {
    pub(crate) projects: Vec<ProjectSnapshot>,
    pub(crate) issues: Vec<ProjectDiscoveryIssue>,
    pub(crate) scanned_entries: u32,
    pub(crate) truncated: bool,
    pub(crate) issues_truncated: bool,
}

#[allow(dead_code)]
pub(crate) struct ProjectStore {
    workspace: PinnedDirectory,
}

#[allow(dead_code)]
impl ProjectStore {
    pub(crate) fn open(workspace_path: &Path) -> Result<Self, ProjectStoreError> {
        probe_workspace(workspace_path)
            .map_err(|_| ProjectStoreError::new("project_workspace_invalid"))?;
        let canonical = fs::canonicalize(workspace_path)
            .map_err(|_| ProjectStoreError::new("project_workspace_invalid"))?;
        let workspace = PinnedDirectory::open(&canonical)?;
        Ok(Self { workspace })
    }

    pub(crate) fn create_project(
        &self,
        project: &Project,
    ) -> Result<ProjectCreateReceipt, ProjectStoreError> {
        self.create_project_with_fault(project, None)
    }

    pub(crate) fn read_project(
        &self,
        locator: &ProjectLocator,
    ) -> Result<ProjectSnapshot, ProjectStoreError> {
        self.read_project_with_recovery(locator)
    }

    pub(crate) fn update_project(
        &self,
        updated: &Project,
        expected_revision: u64,
        expected_fingerprint: ProjectSnapshotFingerprint,
    ) -> Result<ProjectSnapshot, ProjectStoreError> {
        self.update_project_with_fault(updated, expected_revision, expected_fingerprint, None)
    }

    pub(crate) fn discover_projects(&self) -> Result<ProjectDiscoveryReport, ProjectStoreError> {
        self.workspace.revalidate()?;
        let projects_path = self.workspace.path.join("projects");
        if !projects_path.exists() {
            return Ok(ProjectDiscoveryReport {
                projects: Vec::new(),
                issues: Vec::new(),
                scanned_entries: 0,
                truncated: false,
                issues_truncated: false,
            });
        }
        let projects_directory = PinnedDirectory::open(&projects_path)?;
        self.workspace.revalidate()?;

        let mut entries = Vec::new();
        let reader = fs::read_dir(&projects_path)
            .map_err(|_| ProjectStoreError::new("project_discovery_failed"))?;
        for entry in reader {
            let entry = entry.map_err(|_| ProjectStoreError::new("project_discovery_failed"))?;
            entries.push(entry);
            if entries.len() > MAX_DISCOVERY_ENTRIES {
                return Ok(ProjectDiscoveryReport {
                    projects: Vec::new(),
                    issues: vec![ProjectDiscoveryIssue {
                        entry_name: None,
                        code: "project_discovery_limit_exceeded",
                    }],
                    scanned_entries: (MAX_DISCOVERY_ENTRIES + 1) as u32,
                    truncated: true,
                    issues_truncated: false,
                });
            }
        }
        entries.sort_by_key(|entry| entry.file_name());

        let mut report = ProjectDiscoveryReport {
            projects: Vec::new(),
            issues: Vec::new(),
            scanned_entries: entries.len() as u32,
            truncated: false,
            issues_truncated: false,
        };
        let mut candidates = Vec::new();
        for entry in entries {
            let file_name = entry.file_name();
            let entry_name = safe_discovery_entry_name(&file_name);
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    push_discovery_issue(
                        &mut report,
                        entry_name,
                        "project_entry_metadata_unavailable",
                    );
                    continue;
                }
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                push_discovery_issue(&mut report, entry_name, "project_entry_not_directory");
                continue;
            }
            let Some(folder_name) = file_name.to_str() else {
                push_discovery_issue(&mut report, None, "project_entry_name_invalid");
                continue;
            };
            if folder_name.is_empty()
                || folder_name.len() > 128
                || folder_name.chars().any(char::is_control)
            {
                push_discovery_issue(&mut report, entry_name, "project_entry_name_invalid");
                continue;
            }

            match self.read_discovered_project(folder_name) {
                Ok(snapshot) => candidates.push((snapshot, entry_name)),
                Err(error) => push_discovery_issue(&mut report, entry_name, error.code),
            }
        }
        projects_directory.revalidate()?;
        self.workspace.revalidate()?;
        let mut id_counts = HashMap::new();
        for (snapshot, _) in &candidates {
            *id_counts.entry(snapshot.project.id).or_insert(0_u32) += 1;
        }
        for (snapshot, entry_name) in candidates {
            if id_counts.get(&snapshot.project.id) == Some(&1) {
                report.projects.push(snapshot);
            } else {
                push_discovery_issue(&mut report, entry_name, "project_duplicate_id");
            }
        }
        report.projects.sort_by(|left, right| {
            let left_updated = OffsetDateTime::parse(&left.project.updated_at, &Rfc3339).unwrap();
            let right_updated = OffsetDateTime::parse(&right.project.updated_at, &Rfc3339).unwrap();
            let left_created = OffsetDateTime::parse(&left.project.created_at, &Rfc3339).unwrap();
            let right_created = OffsetDateTime::parse(&right.project.created_at, &Rfc3339).unwrap();
            right_updated
                .cmp(&left_updated)
                .then_with(|| right_created.cmp(&left_created))
                .then_with(|| left.project.folder_name.cmp(&right.project.folder_name))
        });
        Ok(report)
    }

    fn read_discovered_project(
        &self,
        folder_name: &str,
    ) -> Result<ProjectSnapshot, ProjectStoreError> {
        self.workspace.revalidate()?;
        let projects_path = self.workspace.path.join("projects");
        let projects = PinnedDirectory::open(&projects_path)?;
        let project_path = projects_path.join(folder_name);
        let project = PinnedDirectory::open(&project_path)?;
        projects.revalidate()?;
        project.revalidate()?;
        let paths = ProjectDocumentPaths::new(&project_path);

        let candidate = match read_locked_snapshot_unbound(&paths.document) {
            Ok(snapshot) => snapshot,
            Err(error) if error.is_recoverable_snapshot_failure() => {
                read_locked_snapshot_unbound(&paths.backup)
                    .map_err(|_| ProjectStoreError::new("project_snapshot_recovery_failed"))?
            }
            Err(error) => return Err(error),
        };
        if candidate.project.folder_name != folder_name {
            return Err(ProjectStoreError::new("project_snapshot_identity_mismatch"));
        }
        let locator = ProjectLocator::from_project(&candidate.project)?;
        drop(candidate);
        drop(project);
        drop(projects);
        self.read_project(&locator)
    }

    fn create_project_with_fault(
        &self,
        project: &Project,
        fault: Option<CreateFault>,
    ) -> Result<ProjectCreateReceipt, ProjectStoreError> {
        let layout = PortableProjectLayout::for_project(project)
            .map_err(|_| ProjectStoreError::new("project_contract_invalid"))?;
        self.workspace.revalidate()?;

        let projects_path = self.workspace.path.join("projects");
        let created_projects = create_directory(&projects_path)?;
        let projects = match PinnedDirectory::open(&projects_path) {
            Ok(directory) => directory,
            Err(error) => {
                if created_projects {
                    let _ = fs::remove_dir(&projects_path);
                }
                return Err(error);
            }
        };
        if let Err(error) = self.workspace.revalidate() {
            drop(projects);
            if created_projects {
                let _ = fs::remove_dir(&projects_path);
            }
            return Err(error);
        }

        let project_path = projects_path.join(&project.folder_name);
        match fs::create_dir(&project_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                drop(projects);
                if created_projects {
                    let _ = fs::remove_dir(&projects_path);
                }
                return Err(ProjectStoreError::new("project_already_exists"));
            }
            Err(_) => {
                drop(projects);
                if created_projects {
                    let _ = fs::remove_dir(&projects_path);
                }
                return Err(ProjectStoreError::new("project_directory_create_failed"));
            }
        }

        let project_directory = match PinnedDirectory::open(&project_path) {
            Ok(directory) => Some(directory),
            Err(error) => {
                let _ = fs::remove_dir(&project_path);
                drop(projects);
                if created_projects {
                    let _ = fs::remove_dir(&projects_path);
                }
                return Err(error);
            }
        };
        let temporary_path = project_path.join(TEMP_PROJECT_DOCUMENT);
        let document_path = project_path.join("project.md");

        let result = (|| {
            inject_fault(fault, CreateFault::ProjectDirectory)?;
            projects.revalidate()?;
            project_directory.as_ref().unwrap().revalidate()?;

            let document = render_project_document(project)?;
            let mut temporary = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(|_| ProjectStoreError::new("project_snapshot_write_failed"))?;
            temporary
                .write_all(document.as_bytes())
                .and_then(|()| temporary.sync_all())
                .map_err(|_| ProjectStoreError::new("project_snapshot_write_failed"))?;
            drop(temporary);

            inject_fault(fault, CreateFault::TempSynced)?;
            project_directory.as_ref().unwrap().revalidate()?;
            atomic_replace(&temporary_path, &document_path)?;
            inject_fault(fault, CreateFault::Published)?;
            project_directory.as_ref().unwrap().revalidate()?;

            Ok(ProjectCreateReceipt {
                relative_document: layout.project_document,
                bytes_written: document.len() as u64,
            })
        })();

        if result.is_err() {
            // The project directory was created exclusively by this invocation and is still
            // pinned against rename, so cleanup cannot traverse a substituted directory.
            let _ = fs::remove_file(&temporary_path);
            let _ = fs::remove_file(&document_path);
        }
        drop(project_directory);
        if result.is_err() {
            let _ = fs::remove_dir(&project_path);
        }
        drop(projects);
        if result.is_err() && created_projects {
            let _ = fs::remove_dir(&projects_path);
        }

        result
    }

    fn read_project_with_recovery(
        &self,
        locator: &ProjectLocator,
    ) -> Result<ProjectSnapshot, ProjectStoreError> {
        let pinned = self.open_existing_project(locator)?;
        let paths = ProjectDocumentPaths::new(&pinned.project.path);
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
                let missing = main.error.code == "project_snapshot_missing";
                let backup = read_locked_snapshot(&paths.backup, locator)
                    .map_err(|_| ProjectStoreError::new("project_snapshot_recovery_failed"))?;
                pinned.revalidate()?;
                publish_recovery_bytes(&paths, &backup.bytes, !missing)?;
                drop(main);
                let restored = read_locked_snapshot(&paths.document, locator)
                    .map_err(|_| ProjectStoreError::new("project_snapshot_recovery_failed"))?;
                Ok(restored.into_public(true))
            }
            Err(error) => Err(error),
        }
    }

    fn update_project_with_fault(
        &self,
        updated: &Project,
        expected_revision: u64,
        expected_fingerprint: ProjectSnapshotFingerprint,
        fault: Option<UpdateFault>,
    ) -> Result<ProjectSnapshot, ProjectStoreError> {
        updated
            .validate()
            .map_err(|_| ProjectStoreError::new("project_contract_invalid"))?;
        let locator = ProjectLocator::from_project(updated)?;
        let pinned = self.open_existing_project(&locator)?;
        let paths = ProjectDocumentPaths::new(&pinned.project.path);
        let current = read_locked_snapshot(&paths.document, &locator)?;

        validate_project_update(
            &current.project,
            updated,
            expected_revision,
            expected_fingerprint,
            current.fingerprint,
        )?;
        let document = render_project_document(updated)?;
        write_synced_temporary(&paths.temporary, document.as_bytes())?;

        let mut replaced = false;
        let result = (|| {
            inject_update_fault(fault, UpdateFault::TemporarySynced)?;
            pinned.revalidate()?;
            remove_if_file(&paths.backup)?;
            atomic_replace_with_backup(&paths.temporary, &paths.document, &paths.backup)?;
            replaced = true;
            inject_update_fault(fault, UpdateFault::Replaced)?;
            pinned.revalidate()?;
            let snapshot = read_locked_snapshot(&paths.document, &locator)?;
            if snapshot.project != *updated {
                return Err(ProjectStoreError::new("project_snapshot_verify_failed"));
            }
            Ok(snapshot.into_public(false))
        })();

        drop(current);
        if result.is_err() {
            let _ = fs::remove_file(&paths.temporary);
            if replaced {
                let _ = restore_backup(&paths, &locator);
            }
        }
        result
    }

    fn open_existing_project(
        &self,
        locator: &ProjectLocator,
    ) -> Result<PinnedProjectDirectory, ProjectStoreError> {
        self.workspace.revalidate()?;
        let projects_path = self.workspace.path.join("projects");
        let projects = PinnedDirectory::open(&projects_path)?;
        let project_path = projects_path.join(&locator.folder_name);
        let project = PinnedDirectory::open(&project_path)?;
        self.workspace.revalidate()?;
        projects.revalidate()?;
        project.revalidate()?;
        Ok(PinnedProjectDirectory { projects, project })
    }
}

struct PinnedProjectDirectory {
    projects: PinnedDirectory,
    project: PinnedDirectory,
}

impl PinnedProjectDirectory {
    fn revalidate(&self) -> Result<(), ProjectStoreError> {
        self.projects.revalidate()?;
        self.project.revalidate()
    }
}

struct ProjectDocumentPaths {
    document: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
}

impl ProjectDocumentPaths {
    fn new(project_directory: &Path) -> Self {
        Self {
            document: project_directory.join("project.md"),
            temporary: project_directory.join(TEMP_PROJECT_DOCUMENT),
            backup: project_directory.join(BACKUP_PROJECT_DOCUMENT),
        }
    }
}

struct LockedProjectSnapshot {
    _file: File,
    bytes: Vec<u8>,
    project: Project,
    fingerprint: ProjectSnapshotFingerprint,
}

#[derive(Debug)]
struct LockedSnapshotReadError {
    error: ProjectStoreError,
    _file: Option<File>,
}

impl LockedProjectSnapshot {
    fn into_public(self, recovered_from_backup: bool) -> ProjectSnapshot {
        ProjectSnapshot {
            project: self.project,
            fingerprint: self.fingerprint,
            recovered_from_backup,
        }
    }
}

fn create_directory(path: &Path) -> Result<bool, ProjectStoreError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => Err(ProjectStoreError::new("project_directory_create_failed")),
    }
}

fn render_project_document(project: &Project) -> Result<String, ProjectStoreError> {
    project
        .validate()
        .map_err(|_| ProjectStoreError::new("project_contract_invalid"))?;
    let json = serde_json::to_value(project)
        .map_err(|_| ProjectStoreError::new("project_snapshot_render_failed"))?;
    let object = json
        .as_object()
        .ok_or_else(|| ProjectStoreError::new("project_snapshot_render_failed"))?;
    let scalar = |name: &str| render_yaml_flow(&object[name]);

    let mut document = String::from("---\n");
    document.push_str("schema_version: ");
    document.push_str(&scalar("schemaVersion")?);
    document.push_str("\ndocument_type: \"project\"\n");
    for (yaml_name, json_name) in [
        ("id", "id"),
        ("name", "name"),
        ("folder_name", "folderName"),
        ("description", "description"),
        ("global_context", "globalContext"),
        ("participants", "participants"),
        ("tags", "tags"),
        ("default_preset_id", "defaultPresetId"),
        (
            "default_transcription_model_id",
            "defaultTranscriptionModelId",
        ),
        ("preferred_llm_models", "preferredLlmModels"),
        ("created_at", "createdAt"),
        ("updated_at", "updatedAt"),
        ("revision", "revision"),
    ] {
        document.push_str(yaml_name);
        document.push_str(": ");
        document.push_str(&scalar(json_name)?);
        document.push('\n');
    }
    document.push_str("---\n");
    Ok(document)
}

fn render_yaml_flow(value: &serde_json::Value) -> Result<String, ProjectStoreError> {
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
                        .map_err(|_| ProjectStoreError::new("project_snapshot_render_failed"))?;
                    Ok(format!("{name}: {}", render_yaml_flow(value)?))
                })
                .collect::<Result<Vec<_>, ProjectStoreError>>()?;
            Ok(format!("{{{}}}", fields.join(", ")))
        }
        _ => serde_json::to_string(value)
            .map_err(|_| ProjectStoreError::new("project_snapshot_render_failed")),
    }
}

fn read_locked_snapshot(
    path: &Path,
    locator: &ProjectLocator,
) -> Result<LockedProjectSnapshot, ProjectStoreError> {
    read_locked_snapshot_internal(path, Some(locator)).map_err(|error| error.error)
}

fn read_locked_snapshot_unbound(path: &Path) -> Result<LockedProjectSnapshot, ProjectStoreError> {
    read_locked_snapshot_internal(path, None).map_err(|error| error.error)
}

fn read_locked_snapshot_internal(
    path: &Path,
    locator: Option<&ProjectLocator>,
) -> Result<LockedProjectSnapshot, LockedSnapshotReadError> {
    let mut file = open_snapshot_without_write_share(path)
        .map_err(|error| LockedSnapshotReadError { error, _file: None })?;
    let length = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => {
            return Err(LockedSnapshotReadError {
                error: ProjectStoreError::new("project_snapshot_read_failed"),
                _file: Some(file),
            });
        }
    };
    if length > MAX_PROJECT_DOCUMENT_BYTES {
        return Err(LockedSnapshotReadError {
            error: ProjectStoreError::new("project_snapshot_too_large"),
            _file: Some(file),
        });
    }
    let capacity = match usize::try_from(length) {
        Ok(capacity) => capacity,
        Err(_) => {
            return Err(LockedSnapshotReadError {
                error: ProjectStoreError::new("project_snapshot_too_large"),
                _file: Some(file),
            });
        }
    };
    let mut bytes = Vec::with_capacity(capacity);
    if Read::by_ref(&mut file)
        .take(MAX_PROJECT_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(LockedSnapshotReadError {
            error: ProjectStoreError::new("project_snapshot_read_failed"),
            _file: Some(file),
        });
    }
    if bytes.len() as u64 > MAX_PROJECT_DOCUMENT_BYTES {
        return Err(LockedSnapshotReadError {
            error: ProjectStoreError::new("project_snapshot_too_large"),
            _file: Some(file),
        });
    }
    let project = match parse_project_document(&bytes) {
        Ok(project) => project,
        Err(error) => {
            return Err(LockedSnapshotReadError {
                error,
                _file: Some(file),
            });
        }
    };
    if locator.is_some_and(|locator| {
        project.id != locator.id || project.folder_name != locator.folder_name
    }) {
        return Err(LockedSnapshotReadError {
            error: ProjectStoreError::new("project_snapshot_identity_mismatch"),
            _file: Some(file),
        });
    }
    let fingerprint = ProjectSnapshotFingerprint(Sha256::digest(&bytes).into());
    Ok(LockedProjectSnapshot {
        _file: file,
        bytes,
        project,
        fingerprint,
    })
}

fn safe_discovery_entry_name(name: &std::ffi::OsStr) -> Option<String> {
    let value = name.to_str()?;
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        None
    } else {
        Some(value.to_owned())
    }
}

fn push_discovery_issue(
    report: &mut ProjectDiscoveryReport,
    entry_name: Option<String>,
    code: &'static str,
) {
    if report.issues.len() < MAX_DISCOVERY_ISSUES {
        report
            .issues
            .push(ProjectDiscoveryIssue { entry_name, code });
    } else {
        report.issues_truncated = true;
    }
}

fn parse_project_document(bytes: &[u8]) -> Result<Project, ProjectStoreError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ProjectStoreError::new("project_snapshot_invalid"))?;
    let normalized = text.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(ProjectStoreError::new("project_snapshot_invalid"));
    }
    let mut lines = normalized.lines();
    if lines.next() != Some("---") {
        return Err(ProjectStoreError::new("project_snapshot_invalid"));
    }

    let mut object = serde_json::Map::new();
    for (yaml_name, json_name) in [
        ("schema_version", "schemaVersion"),
        ("document_type", "documentType"),
        ("id", "id"),
        ("name", "name"),
        ("folder_name", "folderName"),
        ("description", "description"),
        ("global_context", "globalContext"),
        ("participants", "participants"),
        ("tags", "tags"),
        ("default_preset_id", "defaultPresetId"),
        (
            "default_transcription_model_id",
            "defaultTranscriptionModelId",
        ),
        ("preferred_llm_models", "preferredLlmModels"),
        ("created_at", "createdAt"),
        ("updated_at", "updatedAt"),
        ("revision", "revision"),
    ] {
        let line = lines
            .next()
            .ok_or_else(|| ProjectStoreError::new("project_snapshot_invalid"))?;
        let (name, encoded) = line
            .split_once(": ")
            .ok_or_else(|| ProjectStoreError::new("project_snapshot_invalid"))?;
        if name != yaml_name {
            return Err(ProjectStoreError::new("project_snapshot_invalid"));
        }
        let value: serde_json::Value = serde_json::from_str(encoded)
            .map_err(|_| ProjectStoreError::new("project_snapshot_invalid"))?;
        object.insert(json_name.to_owned(), value);
    }
    if lines.next() != Some("---") || lines.next().is_some() {
        return Err(ProjectStoreError::new("project_snapshot_invalid"));
    }
    if object.remove("documentType") != Some(serde_json::json!("project")) {
        return Err(ProjectStoreError::new("project_snapshot_invalid"));
    }
    serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|_| ProjectStoreError::new("project_snapshot_invalid"))
}

fn validate_project_update(
    current: &Project,
    updated: &Project,
    expected_revision: u64,
    expected_fingerprint: ProjectSnapshotFingerprint,
    current_fingerprint: ProjectSnapshotFingerprint,
) -> Result<(), ProjectStoreError> {
    if current.revision != expected_revision {
        return Err(ProjectStoreError::new("project_revision_conflict"));
    }
    if expected_fingerprint != current_fingerprint {
        return Err(ProjectStoreError::new("project_external_modification"));
    }
    if current.id != updated.id
        || current.folder_name != updated.folder_name
        || current.created_at != updated.created_at
    {
        return Err(ProjectStoreError::new("project_snapshot_identity_mismatch"));
    }
    if expected_revision.checked_add(1) != Some(updated.revision) {
        return Err(ProjectStoreError::new("project_revision_invalid"));
    }
    let current_updated = OffsetDateTime::parse(&current.updated_at, &Rfc3339)
        .map_err(|_| ProjectStoreError::new("project_snapshot_invalid"))?;
    let next_updated = OffsetDateTime::parse(&updated.updated_at, &Rfc3339)
        .map_err(|_| ProjectStoreError::new("project_contract_invalid"))?;
    if next_updated < current_updated {
        return Err(ProjectStoreError::new("project_revision_invalid"));
    }
    Ok(())
}

fn write_synced_temporary(path: &Path, bytes: &[u8]) -> Result<(), ProjectStoreError> {
    remove_if_file(path)?;
    let mut temporary = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ProjectStoreError::new("project_snapshot_write_failed"))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.sync_all())
        .map_err(|_| ProjectStoreError::new("project_snapshot_write_failed"))
}

fn publish_recovery_bytes(
    paths: &ProjectDocumentPaths,
    bytes: &[u8],
    replace_existing: bool,
) -> Result<(), ProjectStoreError> {
    write_synced_temporary(&paths.temporary, bytes)?;
    let result = if replace_existing {
        atomic_replace_existing(&paths.temporary, &paths.document)
    } else {
        atomic_publish_new(&paths.temporary, &paths.document)
    };
    result.map_err(|_| ProjectStoreError::new("project_snapshot_recovery_failed"))
}

fn restore_backup(
    paths: &ProjectDocumentPaths,
    locator: &ProjectLocator,
) -> Result<(), ProjectStoreError> {
    let backup = read_locked_snapshot(&paths.backup, locator)?;
    publish_recovery_bytes(paths, &backup.bytes, true)
}

fn remove_if_file(path: &Path) -> Result<(), ProjectStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ProjectStoreError::new("project_snapshot_cleanup_failed")),
    }
}

impl ProjectStoreError {
    fn is_recoverable_snapshot_failure(self) -> bool {
        matches!(
            self.code,
            "project_snapshot_missing" | "project_snapshot_invalid" | "project_snapshot_too_large"
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateFault {
    TemporarySynced,
    Replaced,
}

fn inject_update_fault(
    configured: Option<UpdateFault>,
    current: UpdateFault,
) -> Result<(), ProjectStoreError> {
    if configured == Some(current) {
        Err(ProjectStoreError::new("project_update_fault_injected"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateFault {
    ProjectDirectory,
    TempSynced,
    Published,
}

fn inject_fault(
    configured: Option<CreateFault>,
    current: CreateFault,
) -> Result<(), ProjectStoreError> {
    if configured == Some(current) {
        Err(ProjectStoreError::new("project_create_fault_injected"))
    } else {
        Ok(())
    }
}

struct PinnedDirectory {
    path: PathBuf,
    file: File,
    identity: DirectoryIdentity,
}

impl PinnedDirectory {
    fn open(path: &Path) -> Result<Self, ProjectStoreError> {
        reject_reparse_points(path).map_err(|_| ProjectStoreError::new("project_path_unsafe"))?;
        let file = open_directory_without_delete_share(path)?;
        let identity = directory_identity(&file)?;
        if identity.reparse_point {
            return Err(ProjectStoreError::new("project_path_unsafe"));
        }
        Ok(Self {
            path: path.to_owned(),
            file,
            identity,
        })
    }

    fn revalidate(&self) -> Result<(), ProjectStoreError> {
        reject_reparse_points(&self.path)
            .map_err(|_| ProjectStoreError::new("project_path_unsafe"))?;
        let current = open_directory_without_delete_share(&self.path)?;
        let identity = directory_identity(&current)?;
        if identity != self.identity || identity.reparse_point {
            return Err(ProjectStoreError::new("project_path_identity_changed"));
        }
        let pinned = directory_identity(&self.file)?;
        if pinned != self.identity {
            return Err(ProjectStoreError::new("project_path_identity_changed"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    volume_serial: u32,
    file_index: u64,
    reparse_point: bool,
}

#[cfg(windows)]
fn open_directory_without_delete_share(path: &Path) -> Result<File, ProjectStoreError> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ProjectStoreError::new("project_path_open_failed"))
}

#[cfg(windows)]
fn open_snapshot_without_write_share(path: &Path) -> Result<File, ProjectStoreError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ProjectStoreError::new("project_snapshot_missing")
            } else {
                ProjectStoreError::new("project_snapshot_read_failed")
            }
        })?;
    if directory_identity(&file)?.reparse_point {
        return Err(ProjectStoreError::new("project_path_unsafe"));
    }
    Ok(file)
}

#[cfg(not(windows))]
fn open_snapshot_without_write_share(_path: &Path) -> Result<File, ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(not(windows))]
fn open_directory_without_delete_share(_path: &Path) -> Result<File, ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(windows)]
fn directory_identity(file: &File) -> Result<DirectoryIdentity, ProjectStoreError> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the file owns a valid handle and `information` is writable for the duration.
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as RawHandle, &mut information) };
    if succeeded == 0 {
        return Err(ProjectStoreError::new("project_path_identity_unavailable"));
    }
    Ok(DirectoryIdentity {
        volume_serial: information.dwVolumeSerialNumber,
        file_index: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
        reparse_point: information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
    })
}

#[cfg(not(windows))]
fn directory_identity(_file: &File) -> Result<DirectoryIdentity, ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), ProjectStoreError> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both vectors are NUL-terminated UTF-16 paths alive for the call. The source
    // and destination are fixed names in the same pinned project directory.
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(ProjectStoreError::new("project_snapshot_publish_failed"))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(_source: &Path, _destination: &Path) -> Result<(), ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(windows)]
fn atomic_publish_new(source: &Path, destination: &Path) -> Result<(), ProjectStoreError> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both paths are NUL-terminated UTF-16 values in the same pinned directory.
    // Omitting REPLACE_EXISTING makes recovery fail closed if another writer creates main.
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(ProjectStoreError::new("project_snapshot_publish_failed"))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_publish_new(_source: &Path, _destination: &Path) -> Result<(), ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(windows)]
fn atomic_replace_existing(source: &Path, destination: &Path) -> Result<(), ProjectStoreError> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both paths are NUL-terminated UTF-16 values in the same pinned directory.
    // A null backup preserves the already-validated recovery backup.
    let succeeded = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if succeeded == 0 {
        Err(ProjectStoreError::new("project_snapshot_publish_failed"))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace_existing(_source: &Path, _destination: &Path) -> Result<(), ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(windows)]
fn atomic_replace_with_backup(
    source: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<(), ProjectStoreError> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let backup: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: all paths are NUL-terminated UTF-16 values in the same pinned project
    // directory. The current document handle denies write sharing but permits deletion.
    let succeeded = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            backup.as_ptr(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if succeeded == 0 {
        Err(ProjectStoreError::new("project_snapshot_publish_failed"))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace_with_backup(
    _source: &Path,
    _destination: &Path,
    _backup: &Path,
) -> Result<(), ProjectStoreError> {
    Err(ProjectStoreError::new("project_windows_only"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        path::{Path, PathBuf},
    };

    use super::{
        CreateFault, MAX_PROJECT_DOCUMENT_BYTES, ProjectLocator, ProjectStore,
        TEMP_PROJECT_DOCUMENT, UpdateFault, parse_project_document, render_project_document,
    };
    use crate::domain::Project;

    fn project() -> Project {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["project"].clone()).unwrap()
    }

    fn updated_project() -> Project {
        let mut value = serde_json::to_value(project()).unwrap();
        value["name"] = serde_json::json!("Weekly product review");
        value["description"] = serde_json::json!("Updated by P4-003.");
        value["updatedAt"] = serde_json::json!("2026-08-11T10:00:00Z");
        value["revision"] = serde_json::json!(3);
        serde_json::from_value(value).unwrap()
    }

    fn project_paths(workspace: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let directory = workspace
            .join("projects")
            .join("weekly-product-meetings--aaaaaaaa");
        (
            directory.join("project.md"),
            directory.join(super::BACKUP_PROJECT_DOCUMENT),
            directory.join(TEMP_PROJECT_DOCUMENT),
        )
    }

    #[test]
    fn project_markdown_matches_the_golden_snapshot() {
        let document = render_project_document(&project()).unwrap();
        assert_eq!(
            document,
            include_str!("../../../fixtures/persistence/project-v1.md")
        );
    }

    #[test]
    fn yaml_scalars_cannot_escape_the_front_matter_shape() {
        let mut value = serde_json::to_value(project()).unwrap();
        value["description"] = serde_json::json!("---\nname: !!unsafe payload");
        value["globalContext"] = serde_json::json!("quoted: \"value\"\n# not a comment");
        let project: Project = serde_json::from_value(value).unwrap();
        let document = render_project_document(&project).unwrap();

        assert_eq!(document.matches("\n---\n").count(), 1);
        assert!(document.contains("description: \"---\\nname: !!unsafe payload\""));
        assert!(document.contains("global_context: \"quoted: \\\"value\\\"\\n# not a comment\""));
    }

    #[test]
    fn strict_front_matter_parser_round_trips_lf_and_crlf() {
        let document = render_project_document(&project()).unwrap();
        assert_eq!(
            parse_project_document(document.as_bytes()).unwrap(),
            project()
        );
        let crlf = document.replace('\n', "\r\n");
        assert_eq!(parse_project_document(crlf.as_bytes()).unwrap(), project());
    }

    #[test]
    fn strict_front_matter_parser_rejects_shape_and_payload_changes() {
        let document = render_project_document(&project()).unwrap();
        for invalid in [
            document.replacen("name: ", "unknown: ", 1),
            document.replacen("name: ", "id: ", 1),
            document.replacen("name: \"", "name: !!tag \"", 1),
            format!("{document}# trailing body\n"),
            document.replacen(
                "document_type: \"project\"",
                "document_type: \"session\"",
                1,
            ),
        ] {
            assert_eq!(
                parse_project_document(invalid.as_bytes()).unwrap_err().code,
                "project_snapshot_invalid"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn creation_publishes_only_the_derived_project_snapshot() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        let receipt = store.create_project(&project()).unwrap();
        let project_directory = workspace
            .path()
            .join("projects")
            .join("weekly-product-meetings--aaaaaaaa");

        assert_eq!(
            receipt.relative_document,
            "projects/weekly-product-meetings--aaaaaaaa/project.md"
        );
        assert_eq!(
            receipt.bytes_written,
            fs::metadata(project_directory.join("project.md"))
                .unwrap()
                .len()
        );
        assert_eq!(project_directory.read_dir().unwrap().count(), 1);
        assert!(
            !project_directory
                .join(super::TEMP_PROJECT_DOCUMENT)
                .exists()
        );
    }

    #[cfg(windows)]
    #[test]
    fn duplicate_creation_never_replaces_an_existing_snapshot() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        store.create_project(&project()).unwrap();
        let document = workspace
            .path()
            .join("projects/weekly-product-meetings--aaaaaaaa/project.md");
        let before = fs::read(&document).unwrap();

        let error = store.create_project(&project()).unwrap_err();
        assert_eq!(error.code, "project_already_exists");
        assert_eq!(fs::read(document).unwrap(), before);
    }

    #[cfg(windows)]
    #[test]
    fn every_injected_fault_cleans_up_owned_artifacts() {
        for fault in [
            CreateFault::ProjectDirectory,
            CreateFault::TempSynced,
            CreateFault::Published,
        ] {
            let workspace = tempfile::tempdir().unwrap();
            let store = ProjectStore::open(workspace.path()).unwrap();
            let error = store
                .create_project_with_fault(&project(), Some(fault))
                .unwrap_err();
            assert_eq!(error.code, "project_create_fault_injected");
            assert_eq!(workspace.path().read_dir().unwrap().count(), 0, "{fault:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn fault_cleanup_preserves_a_preexisting_projects_directory() {
        let workspace = tempfile::tempdir().unwrap();
        fs::create_dir(workspace.path().join("projects")).unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();

        store
            .create_project_with_fault(&project(), Some(CreateFault::TempSynced))
            .unwrap_err();

        let projects = workspace.path().join("projects");
        assert!(projects.is_dir());
        assert_eq!(projects.read_dir().unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn the_workspace_handle_prevents_path_replacement_while_the_store_is_open() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        let moved = root.path().join("moved");
        fs::create_dir(&workspace).unwrap();
        let store = ProjectStore::open(&workspace).unwrap();

        assert!(fs::rename(&workspace, &moved).is_err());
        store.create_project(&project()).unwrap();
        assert!(workspace.join("projects").is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn a_reparse_backed_projects_directory_is_rejected() {
        let workspace = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        junction::create(target.path(), workspace.path().join("projects")).unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();

        let error = store.create_project(&project()).unwrap_err();
        assert_eq!(error.code, "project_path_unsafe");
        assert_eq!(target.path().read_dir().unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn update_requires_revision_and_fingerprint_and_keeps_a_valid_backup() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        store.create_project(&project()).unwrap();
        let locator = ProjectLocator::from_project(&project()).unwrap();
        let initial = store.read_project(&locator).unwrap();

        let updated = store
            .update_project(
                &updated_project(),
                initial.project.revision,
                initial.fingerprint,
            )
            .unwrap();
        let (document, backup, temporary) = project_paths(workspace.path());

        assert_eq!(updated.project, updated_project());
        assert!(!updated.recovered_from_backup);
        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            render_project_document(&project()).unwrap()
        );
        assert_eq!(
            fs::read_to_string(document).unwrap(),
            render_project_document(&updated_project()).unwrap()
        );
        assert!(!temporary.exists());
    }

    #[cfg(windows)]
    #[test]
    fn external_edits_and_revision_conflicts_never_get_overwritten() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        store.create_project(&project()).unwrap();
        let locator = ProjectLocator::from_project(&project()).unwrap();
        let initial = store.read_project(&locator).unwrap();
        let (document, _, _) = project_paths(workspace.path());

        let mut external_value = serde_json::to_value(project()).unwrap();
        external_value["description"] = serde_json::json!("External edit at the same revision");
        let external: Project = serde_json::from_value(external_value).unwrap();
        let external_bytes = render_project_document(&external).unwrap();
        fs::write(&document, &external_bytes).unwrap();

        let error = store
            .update_project(&updated_project(), 2, initial.fingerprint)
            .unwrap_err();
        assert_eq!(error.code, "project_external_modification");
        assert_eq!(fs::read_to_string(&document).unwrap(), external_bytes);

        let external_snapshot = store.read_project(&locator).unwrap();
        let error = store
            .update_project(&updated_project(), 1, external_snapshot.fingerprint)
            .unwrap_err();
        assert_eq!(error.code, "project_revision_conflict");
        assert_eq!(fs::read_to_string(document).unwrap(), external_bytes);
    }

    #[cfg(windows)]
    #[test]
    fn immutable_identity_and_revision_progression_are_enforced() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        store.create_project(&project()).unwrap();
        let locator = ProjectLocator::from_project(&project()).unwrap();
        let initial = store.read_project(&locator).unwrap();

        let mut identity_value = serde_json::to_value(updated_project()).unwrap();
        identity_value["createdAt"] = serde_json::json!("2026-08-10T08:00:00Z");
        let identity_change: Project = serde_json::from_value(identity_value).unwrap();
        assert_eq!(
            store
                .update_project(&identity_change, 2, initial.fingerprint)
                .unwrap_err()
                .code,
            "project_snapshot_identity_mismatch"
        );

        let mut revision_value = serde_json::to_value(updated_project()).unwrap();
        revision_value["revision"] = serde_json::json!(4);
        let skipped_revision: Project = serde_json::from_value(revision_value).unwrap();
        assert_eq!(
            store
                .update_project(&skipped_revision, 2, initial.fingerprint)
                .unwrap_err()
                .code,
            "project_revision_invalid"
        );
    }

    #[cfg(windows)]
    #[test]
    fn valid_backup_recovers_missing_malformed_and_oversized_documents() {
        for failure in ["missing", "malformed", "oversized"] {
            let workspace = tempfile::tempdir().unwrap();
            let store = ProjectStore::open(workspace.path()).unwrap();
            store.create_project(&project()).unwrap();
            let locator = ProjectLocator::from_project(&project()).unwrap();
            let initial = store.read_project(&locator).unwrap();
            store
                .update_project(&updated_project(), 2, initial.fingerprint)
                .unwrap();
            let (document, backup, _) = project_paths(workspace.path());
            let backup_bytes = fs::read(&backup).unwrap();

            match failure {
                "missing" => fs::remove_file(&document).unwrap(),
                "malformed" => fs::write(&document, b"---\ntorn").unwrap(),
                "oversized" => fs::write(
                    &document,
                    vec![b'x'; usize::try_from(MAX_PROJECT_DOCUMENT_BYTES + 1).unwrap()],
                )
                .unwrap(),
                _ => unreachable!(),
            }

            let recovered = store
                .read_project(&locator)
                .unwrap_or_else(|error| panic!("{failure}: {error:?}"));
            assert!(recovered.recovered_from_backup, "{failure}");
            assert_eq!(recovered.project, project(), "{failure}");
            assert_eq!(fs::read(&document).unwrap(), backup_bytes, "{failure}");
            assert_eq!(fs::read(&backup).unwrap(), backup_bytes, "{failure}");
        }
    }

    #[test]
    fn discovery_is_direct_deterministic_and_reports_recovery_and_invalid_entries() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        store.create_project(&project()).unwrap();
        let locator = ProjectLocator::from_project(&project()).unwrap();
        let initial = store.read_project(&locator).unwrap();
        store
            .update_project(&updated_project(), 2, initial.fingerprint)
            .unwrap();
        let (document, _, _) = project_paths(workspace.path());
        fs::write(&document, b"not a project document").unwrap();

        let projects = workspace.path().join("projects");
        fs::write(projects.join("unexpected.txt"), b"ignored").unwrap();
        fs::create_dir(projects.join("malformed--bbbbbbbb")).unwrap();
        fs::write(
            projects.join("malformed--bbbbbbbb").join("project.md"),
            b"invalid",
        )
        .unwrap();
        fs::create_dir_all(
            projects
                .join("container--cccccccc")
                .join("nested--dddddddd"),
        )
        .unwrap();

        let report = store.discover_projects().unwrap();

        assert_eq!(report.scanned_entries, 4);
        assert!(!report.truncated);
        assert!(!report.issues_truncated);
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0].project, project());
        assert!(report.projects[0].recovered_from_backup);
        assert_eq!(
            report
                .issues
                .iter()
                .map(|issue| (issue.entry_name.as_deref(), issue.code))
                .collect::<Vec<_>>(),
            vec![
                (
                    Some("container--cccccccc"),
                    "project_snapshot_recovery_failed"
                ),
                (
                    Some("malformed--bbbbbbbb"),
                    "project_snapshot_recovery_failed"
                ),
                (Some("unexpected.txt"), "project_entry_not_directory"),
            ]
        );
    }

    #[test]
    fn discovery_excludes_every_folder_that_claims_a_duplicate_project_id() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        let first = project();
        let mut value = serde_json::to_value(&first).unwrap();
        value["name"] = serde_json::json!("Duplicate identity");
        value["folderName"] = serde_json::json!("duplicate-identity--aaaaaaaa");
        let duplicate: Project = serde_json::from_value(value).unwrap();
        store.create_project(&first).unwrap();
        store.create_project(&duplicate).unwrap();

        let report = store.discover_projects().unwrap();

        assert!(report.projects.is_empty());
        assert_eq!(report.issues.len(), 2);
        assert!(
            report
                .issues
                .iter()
                .all(|issue| issue.code == "project_duplicate_id")
        );
    }

    #[test]
    fn discovery_limits_fail_closed_without_returning_a_partial_catalog() {
        let workspace = tempfile::tempdir().unwrap();
        let projects = workspace.path().join("projects");
        fs::create_dir(&projects).unwrap();
        for index in 0..=super::MAX_DISCOVERY_ENTRIES {
            File::create(projects.join(format!("entry-{index:04}.txt"))).unwrap();
        }
        let store = ProjectStore::open(workspace.path()).unwrap();

        let report = store.discover_projects().unwrap();

        assert!(report.projects.is_empty());
        assert!(report.truncated);
        assert_eq!(
            report.scanned_entries,
            (super::MAX_DISCOVERY_ENTRIES + 1) as u32
        );
        assert_eq!(report.issues[0].code, "project_discovery_limit_exceeded");
    }

    #[test]
    fn discovery_issue_details_are_bounded() {
        let workspace = tempfile::tempdir().unwrap();
        let projects = workspace.path().join("projects");
        fs::create_dir(&projects).unwrap();
        for index in 0..=super::MAX_DISCOVERY_ISSUES {
            File::create(projects.join(format!("unexpected-{index:03}.txt"))).unwrap();
        }
        let store = ProjectStore::open(workspace.path()).unwrap();

        let report = store.discover_projects().unwrap();

        assert_eq!(report.issues.len(), super::MAX_DISCOVERY_ISSUES);
        assert!(report.issues_truncated);
        assert!(!report.truncated);
    }

    #[cfg(windows)]
    #[test]
    fn unrecoverable_input_is_preserved_and_torn_temp_is_cleaned_only_when_main_is_valid() {
        let workspace = tempfile::tempdir().unwrap();
        let store = ProjectStore::open(workspace.path()).unwrap();
        store.create_project(&project()).unwrap();
        let locator = ProjectLocator::from_project(&project()).unwrap();
        let (document, _, temporary) = project_paths(workspace.path());

        fs::write(&temporary, b"torn temp").unwrap();
        store.read_project(&locator).unwrap();
        assert!(!temporary.exists());

        let malformed = b"---\ntorn";
        fs::write(&document, malformed).unwrap();
        let error = store.read_project(&locator).unwrap_err();
        assert_eq!(error.code, "project_snapshot_recovery_failed");
        assert_eq!(fs::read(document).unwrap(), malformed);
    }

    #[cfg(windows)]
    #[test]
    fn update_faults_leave_the_last_acknowledged_snapshot_readable() {
        for fault in [UpdateFault::TemporarySynced, UpdateFault::Replaced] {
            let workspace = tempfile::tempdir().unwrap();
            let store = ProjectStore::open(workspace.path()).unwrap();
            store.create_project(&project()).unwrap();
            let locator = ProjectLocator::from_project(&project()).unwrap();
            let initial = store.read_project(&locator).unwrap();

            let error = store
                .update_project_with_fault(&updated_project(), 2, initial.fingerprint, Some(fault))
                .unwrap_err();
            assert_eq!(error.code, "project_update_fault_injected");
            let current = store.read_project(&locator).unwrap();
            assert_eq!(current.project, project(), "{fault:?}");
            assert!(!project_paths(workspace.path()).2.exists(), "{fault:?}");
        }
    }
}
