use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::os::windows::{
    ffi::OsStrExt,
    fs::OpenOptionsExt,
    io::{AsRawHandle, RawHandle},
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

use crate::{
    domain::Project,
    security::{probe_workspace, reject_reparse_points},
};

use super::layout::PortableProjectLayout;

const TEMP_PROJECT_DOCUMENT: &str = ".project.md.tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectStoreError {
    pub(crate) code: &'static str,
}

impl ProjectStoreError {
    fn new(code: &'static str) -> Self {
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{CreateFault, ProjectStore, render_project_document};
    use crate::domain::Project;

    fn project() -> Project {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["project"].clone()).unwrap()
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
}
