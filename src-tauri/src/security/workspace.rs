use std::{
    fs,
    io::Write,
    path::{Component, Path, Prefix},
};

use fs2::available_space;
use tempfile::NamedTempFile;
#[cfg(windows)]
use windows_sys::Win32::{
    Storage::FileSystem::GetDriveTypeW, System::WindowsProgramming::DRIVE_FIXED,
};

use crate::domain::{AppError, WorkspaceStatus};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const LOW_DISK_WARNING_BYTES: u64 = 1024 * 1024 * 1024;
const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

pub(crate) fn validate_workspace_path_syntax(path: &Path) -> Result<(), AppError> {
    let raw = path.to_string_lossy();
    if raw.encode_utf16().count() > 32_767
        || raw.contains('/')
        || raw.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return Err(AppError::workspace_invalid(
            "The workspace path contains unsupported Windows path characters.",
        ));
    }
    if !path.is_absolute() {
        return Err(AppError::workspace_invalid(
            "The workspace path must be an absolute local Windows path.",
        ));
    }

    let mut saw_local_disk_prefix = false;
    let mut normal_component_count = 0;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                if matches!(prefix.kind(), Prefix::Disk(_)) {
                    saw_local_disk_prefix = true;
                } else {
                    return Err(AppError::workspace_invalid(
                        "Network, device, and verbatim workspace paths are not supported.",
                    ));
                }
            }
            Component::ParentDir | Component::CurDir => {
                return Err(AppError::workspace_invalid(
                    "The workspace path cannot contain traversal components.",
                ));
            }
            Component::Normal(value) => {
                normal_component_count += 1;
                let value = value.to_string_lossy();
                if value.contains(':') {
                    return Err(AppError::workspace_invalid(
                        "The workspace path cannot contain an alternate data stream.",
                    ));
                }
                if value.ends_with('.') || value.ends_with(' ') || is_reserved_windows_name(&value)
                {
                    return Err(AppError::workspace_invalid(
                        "The workspace path contains a reserved Windows name.",
                    ));
                }
            }
            _ => {}
        }
    }

    if !saw_local_disk_prefix {
        return Err(AppError::workspace_invalid(
            "The workspace path must be on a local Windows drive.",
        ));
    }
    if normal_component_count == 0 {
        return Err(AppError::workspace_invalid(
            "A drive root cannot be used as the workspace.",
        ));
    }

    Ok(())
}

pub(crate) fn probe_workspace(path: &Path) -> Result<WorkspaceStatus, AppError> {
    validate_workspace_path_syntax(path)?;

    let metadata = fs::metadata(path).map_err(|_| {
        AppError::workspace_invalid("The selected workspace folder does not exist.")
    })?;
    ensure_fixed_local_volume(path)?;
    if !metadata.is_dir() {
        return Err(AppError::workspace_invalid(
            "The selected workspace path is not a directory.",
        ));
    }

    reject_reparse_points(path)?;
    let canonical_path = fs::canonicalize(path).map_err(|_| {
        AppError::workspace_invalid("The selected workspace folder could not be resolved.")
    })?;
    reject_reparse_points(&canonical_path)?;

    let mut probe =
        NamedTempFile::new_in(&canonical_path).map_err(|_| AppError::workspace_unwritable())?;
    probe
        .write_all(b"KokoroKoe workspace write probe\n")
        .and_then(|()| probe.as_file().sync_all())
        .map_err(|_| AppError::workspace_unwritable())?;
    probe
        .close()
        .map_err(|_| AppError::workspace_unwritable())?;

    let free_bytes = available_space(&canonical_path)
        .map_err(|_| AppError::workspace_unwritable())?
        .min(JSON_SAFE_INTEGER_MAX);
    let display_path = display_canonical_path(&canonical_path)?;
    let warning = (free_bytes < LOW_DISK_WARNING_BYTES)
        .then(|| "Less than 1 GiB is available in the selected workspace.".to_owned());

    Ok(WorkspaceStatus {
        path: display_path,
        writable: true,
        free_bytes,
        warning,
    })
}

pub(crate) fn prepare_foundation_workspace(path: &Path) -> Result<WorkspaceStatus, AppError> {
    validate_workspace_path_syntax(path)?;
    if path.exists() {
        return probe_workspace(path);
    }
    if path.file_name().and_then(|value| value.to_str()) != Some("KokoroKoe") {
        return Err(AppError::workspace_invalid(
            "Only the known default workspace leaf may be created automatically.",
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        AppError::workspace_invalid("The default workspace parent is unavailable.")
    })?;
    probe_workspace(parent)?;

    let created = match fs::create_dir(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => return Err(AppError::workspace_unwritable()),
    };
    match probe_workspace(path) {
        Ok(status) => Ok(status),
        Err(error) => {
            if created {
                let _ = fs::remove_dir(path);
            }
            Err(error)
        }
    }
}

fn is_reserved_windows_name(value: &str) -> bool {
    let base = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

#[cfg(windows)]
fn ensure_fixed_local_volume(path: &Path) -> Result<(), AppError> {
    let drive = path.components().find_map(|component| match component {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(letter) => Some(letter),
            _ => None,
        },
        _ => None,
    });
    let drive = drive.ok_or_else(|| {
        AppError::workspace_invalid("The workspace path must be on a local Windows drive.")
    })?;
    let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];

    // SAFETY: `root` is a fixed-size, NUL-terminated UTF-16 drive-root string.
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
    if drive_type != DRIVE_FIXED {
        return Err(AppError::workspace_invalid(
            "The workspace must be stored on a fixed local drive.",
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn ensure_fixed_local_volume(_path: &Path) -> Result<(), AppError> {
    Err(AppError::workspace_invalid(
        "Workspace selection is supported only on Windows.",
    ))
}

fn display_canonical_path(path: &Path) -> Result<String, AppError> {
    let display = path.to_str().ok_or_else(|| {
        AppError::workspace_invalid("The selected workspace path is not valid Unicode.")
    })?;

    Ok(display.strip_prefix(r"\\?\").unwrap_or(display).to_owned())
}

pub(crate) fn reject_reparse_points(path: &Path) -> Result<(), AppError> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| {
            AppError::workspace_invalid("The workspace path metadata could not be verified.")
        })?;

        if metadata.file_type().is_symlink() || has_reparse_attribute(&metadata) {
            return Err(AppError::workspace_invalid(
                "The workspace path cannot pass through a link or reparse point.",
            ));
        }
    }

    Ok(())
}

#[cfg(windows)]
fn has_reparse_attribute(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn has_reparse_attribute(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{probe_workspace, validate_workspace_path_syntax};

    #[test]
    fn workspace_syntax_rejects_relative_traversal_ads_and_non_local_paths() {
        for path in [
            r"relative\workspace",
            r"C:\safe\..\escape",
            r"C:\safe\file:stream",
            r"C:\CON\workspace",
            r"C:\safe.\workspace",
            r"C:/safe/workspace",
            r"C:\",
            r"\\server\share\workspace",
            r"\\?\C:\workspace",
        ] {
            assert!(
                validate_workspace_path_syntax(path.as_ref()).is_err(),
                "{path} must be rejected"
            );
        }
    }

    #[test]
    fn workspace_probe_returns_a_canonical_writable_directory() {
        let directory = tempfile::tempdir().expect("temporary directory should be available");
        let status = probe_workspace(directory.path()).expect("temporary directory should pass");

        assert!(status.writable);
        assert!(status.free_bytes > 0);
        assert!(!status.path.starts_with(r"\\?\"));
        assert_eq!(directory.path().read_dir().unwrap().count(), 0);
    }

    #[test]
    fn workspace_probe_rejects_files() {
        let file = tempfile::NamedTempFile::new().expect("temporary file should be available");
        let error = probe_workspace(file.path()).expect_err("a file is not a workspace");

        assert_eq!(error.code, "workspace_invalid");
    }

    #[test]
    fn foundation_workspace_creates_only_the_known_leaf() {
        let documents = tempfile::tempdir().expect("documents should be available");
        let workspace = documents.path().join("KokoroKoe");
        let status = super::prepare_foundation_workspace(&workspace)
            .expect("known foundation leaf should be created and probed");

        assert!(workspace.is_dir());
        assert!(status.writable);
        assert_eq!(workspace.read_dir().unwrap().count(), 0);
        assert!(super::prepare_foundation_workspace(&documents.path().join("Unexpected")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn workspace_probe_rejects_a_reparse_point_in_the_component_chain() {
        use std::fs;

        let root = tempfile::tempdir().expect("temporary root should be available");
        let target = root.path().join("target");
        let nested = target.join("nested");
        fs::create_dir_all(&nested).expect("target directories should be created");
        let link = root.path().join("linked");
        junction::create(&target, &link).expect("test junction should be created");

        let error = probe_workspace(&link.join("nested"))
            .expect_err("a reparse-backed ancestor must be rejected");
        assert_eq!(error.code, "workspace_invalid");
    }
}
