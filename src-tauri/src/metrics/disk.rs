use std::{fs, path::Path};

/// How many directory entries one measurement may visit before it stops and
/// reports a floor. A workspace that large still gets a number, just not a
/// claim that the number is the whole folder.
const MAX_DISK_ENTRIES: usize = 250_000;
const TRASH_DIRECTORY: &str = "trash";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct DiskMeasurement {
    pub(crate) bytes: u64,
    /// The part of `bytes` under the workspace's `trash/` folder.
    pub(crate) trash_bytes: u64,
    pub(crate) complete: bool,
}

/// Sums the sizes of the regular files beneath `root`.
///
/// It reads metadata only, never contents, and never follows a symbolic link
/// or junction: a link out of the workspace would count someone else's files,
/// and a link back into it would count the same ones twice. A folder that
/// cannot be read — removed mid-walk, or locked — is skipped and the result is
/// marked incomplete rather than failing the whole dashboard.
pub(crate) fn measure_directory(root: &Path) -> DiskMeasurement {
    let mut measurement = DiskMeasurement {
        complete: true,
        ..DiskMeasurement::default()
    };
    if !root.is_dir() {
        return measurement;
    }
    let mut pending = vec![(root.to_path_buf(), false)];
    let mut visited = 0_usize;
    while let Some((directory, in_trash)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            measurement.complete = false;
            continue;
        };
        for entry in entries {
            visited += 1;
            if visited > MAX_DISK_ENTRIES {
                measurement.complete = false;
                return measurement;
            }
            let Ok(entry) = entry else {
                measurement.complete = false;
                continue;
            };
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                measurement.complete = false;
                continue;
            };
            if is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                let trash = in_trash || (directory == root && entry.file_name() == TRASH_DIRECTORY);
                pending.push((entry.path(), trash));
            } else if metadata.is_file() {
                measurement.bytes = measurement.bytes.saturating_add(metadata.len());
                if in_trash {
                    measurement.trash_bytes =
                        measurement.trash_bytes.saturating_add(metadata.len());
                }
            }
        }
    }
    measurement
}

#[cfg(windows)]
fn is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_add_up_and_the_trash_is_counted_within_them() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("projects/a/sessions/s")).unwrap();
        fs::write(root.path().join("projects/a/project.md"), [0_u8; 100]).unwrap();
        fs::write(
            root.path().join("projects/a/sessions/s/transcript.md"),
            [0_u8; 50],
        )
        .unwrap();
        fs::create_dir_all(root.path().join("trash/projects/b")).unwrap();
        fs::write(root.path().join("trash/projects/b/project.md"), [0_u8; 25]).unwrap();
        // A folder merely named like the trash, deeper down, is not the trash.
        fs::create_dir_all(root.path().join("projects/a/trash")).unwrap();
        fs::write(root.path().join("projects/a/trash/note.md"), [0_u8; 5]).unwrap();

        let measured = measure_directory(root.path());
        assert_eq!(measured.bytes, 180);
        assert_eq!(measured.trash_bytes, 25);
        assert!(measured.complete);
    }

    #[test]
    fn a_missing_folder_measures_as_empty() {
        let root = tempfile::tempdir().unwrap();
        let measured = measure_directory(&root.path().join("absent"));
        assert_eq!(
            measured,
            DiskMeasurement {
                bytes: 0,
                trash_bytes: 0,
                complete: true,
            }
        );
    }
}
