use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::{CONTENT_LENGTH, CONTENT_RANGE, ETAG, LAST_MODIFIED, RANGE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ModelDescriptor, catalog};

const DISK_SAFETY_BYTES: u64 = 64 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const MAX_RESUME_METADATA_BYTES: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Compatibility {
    pub available_disk_bytes: u64,
    pub required_disk_bytes: u64,
    pub available_memory_bytes: u64,
    pub approximate_memory_bytes: u64,
    pub disk_compatible: bool,
    pub memory_compatible: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    Installed,
    AlreadyInstalled,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallProgress {
    Downloading {
        bytes_downloaded: u64,
        total_bytes: u64,
    },
    Verifying {
        total_bytes: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelManagerError {
    UnknownModel,
    ModelRootUnavailable,
    UnsafeModelRoot,
    DiskSpaceUnavailable,
    InsufficientDisk,
    DownloadUnavailable,
    DownloadProtocolInvalid,
    DownloadInterrupted,
    StagedModelInvalid,
    InstalledModelInvalid,
    ModelNotInstalled,
    SelectedModelCannotBeDeleted,
    FileOperationFailed,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResumeMetadata {
    model_id: String,
    source_revision: String,
    expected_bytes: u64,
    sha256: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl ResumeMetadata {
    fn matches(&self, descriptor: &ModelDescriptor) -> bool {
        self.model_id == descriptor.id
            && self.source_revision == descriptor.source_revision
            && self.expected_bytes == descriptor.download_bytes
            && self.sha256 == descriptor.sha256
    }
}

pub struct ModelManager {
    root: PathBuf,
    selected_model_id: Option<&'static str>,
}

impl ModelManager {
    pub fn open_app_local(app_local_data_directory: &Path) -> Result<Self, ModelManagerError> {
        Self::open_root(app_local_data_directory.join("models"))
    }

    fn open_root(root: PathBuf) -> Result<Self, ModelManagerError> {
        fs::create_dir_all(&root).map_err(|_| ModelManagerError::ModelRootUnavailable)?;
        reject_reparse_root(&root)?;
        let root = fs::canonicalize(root).map_err(|_| ModelManagerError::ModelRootUnavailable)?;
        Ok(Self {
            root,
            selected_model_id: None,
        })
    }

    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn compatibility(&self, model_id: &str) -> Result<Compatibility, ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        let staged_bytes = self.valid_staged_bytes(descriptor)?;
        let required_disk_bytes = descriptor
            .download_bytes
            .saturating_sub(staged_bytes)
            .saturating_add(DISK_SAFETY_BYTES);
        let available_disk_bytes = fs2::available_space(&self.root)
            .map_err(|_| ModelManagerError::DiskSpaceUnavailable)?;
        let available_memory_bytes = available_memory_bytes();
        Ok(compatibility_from(
            descriptor,
            available_disk_bytes,
            required_disk_bytes,
            available_memory_bytes,
        ))
    }

    pub fn install_with_progress(
        &mut self,
        model_id: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(InstallProgress),
    ) -> Result<InstallOutcome, ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        let compatibility = self.compatibility(model_id)?;
        if !compatibility.disk_compatible {
            return Err(ModelManagerError::InsufficientDisk);
        }
        let source = HttpDownloadSource::new()?;
        self.install_from_with_progress(
            descriptor,
            &source,
            cancel,
            compatibility.available_disk_bytes,
            &mut progress,
        )
    }

    pub fn resumable_bytes(&self, model_id: &str) -> Result<u64, ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        self.valid_staged_bytes(descriptor)
    }

    pub fn is_installed(&self, model_id: &str) -> Result<bool, ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        let final_path = self.final_path(descriptor);
        if !final_path.exists() {
            return Ok(false);
        }
        verify_file(&final_path, descriptor).map(|_| true)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn verified_model_path(&self, model_id: &str) -> Result<PathBuf, ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        let final_path = self.final_path(descriptor);
        if !final_path.exists() {
            return Err(ModelManagerError::ModelNotInstalled);
        }
        let canonical =
            fs::canonicalize(final_path).map_err(|_| ModelManagerError::InstalledModelInvalid)?;
        if canonical.parent() != Some(self.root.as_path()) {
            return Err(ModelManagerError::UnsafeModelRoot);
        }
        verify_file(&canonical, descriptor)?;
        Ok(canonical)
    }

    pub fn select(&mut self, model_id: &str) -> Result<(), ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        self.select_descriptor(descriptor)
    }

    fn select_descriptor(
        &mut self,
        descriptor: &'static ModelDescriptor,
    ) -> Result<(), ModelManagerError> {
        if !self.final_path(descriptor).exists() {
            return Err(ModelManagerError::ModelNotInstalled);
        }
        verify_file(&self.final_path(descriptor), descriptor)?;
        self.selected_model_id = Some(descriptor.id);
        Ok(())
    }

    #[cfg(test)]
    pub fn selected_model_id(&self) -> Option<&'static str> {
        self.selected_model_id
    }

    #[cfg(test)]
    pub fn clear_selection(&mut self) {
        self.selected_model_id = None;
    }

    pub fn delete(&mut self, model_id: &str) -> Result<u64, ModelManagerError> {
        let descriptor = catalog::find(model_id).ok_or(ModelManagerError::UnknownModel)?;
        self.delete_descriptor(descriptor)
    }

    fn delete_descriptor(&self, descriptor: &ModelDescriptor) -> Result<u64, ModelManagerError> {
        if self.selected_model_id == Some(descriptor.id) {
            return Err(ModelManagerError::SelectedModelCannotBeDeleted);
        }

        let mut reclaimed = 0_u64;
        for path in self.model_paths(descriptor) {
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    reclaimed = reclaimed.saturating_add(metadata.len());
                    fs::remove_file(path).map_err(|_| ModelManagerError::FileOperationFailed)?;
                }
                Ok(_) => return Err(ModelManagerError::UnsafeModelRoot),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return Err(ModelManagerError::FileOperationFailed),
            }
        }
        Ok(reclaimed)
    }

    fn install_from_with_progress(
        &self,
        descriptor: &ModelDescriptor,
        source: &dyn DownloadSource,
        cancel: &AtomicBool,
        available_disk_bytes: u64,
        progress: &mut dyn FnMut(InstallProgress),
    ) -> Result<InstallOutcome, ModelManagerError> {
        let final_path = self.final_path(descriptor);
        if final_path.exists() {
            if verify_file(&final_path, descriptor).is_ok() {
                return Ok(InstallOutcome::AlreadyInstalled);
            }
            remove_exact_file(&final_path)?;
        }

        self.normalize_staging(descriptor)?;
        let part_path = self.part_path(descriptor);
        let mut offset = file_len_or_zero(&part_path)?;
        if offset == descriptor.download_bytes {
            if verify_file(&part_path, descriptor).is_ok() {
                progress(InstallProgress::Verifying {
                    total_bytes: descriptor.download_bytes,
                });
                if cancel.load(Ordering::Relaxed) {
                    return Ok(InstallOutcome::Cancelled);
                }
                OpenOptions::new()
                    .write(true)
                    .open(&part_path)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| ModelManagerError::FileOperationFailed)?;
                fs::rename(&part_path, &final_path)
                    .map_err(|_| ModelManagerError::FileOperationFailed)?;
                remove_if_present(&self.metadata_path(descriptor))?;
                return Ok(InstallOutcome::Installed);
            }
            self.clear_staging(descriptor)?;
            offset = 0;
        }
        let required = descriptor
            .download_bytes
            .saturating_sub(offset)
            .saturating_add(DISK_SAFETY_BYTES);
        if available_disk_bytes < required {
            return Err(ModelManagerError::InsufficientDisk);
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(InstallOutcome::Cancelled);
        }

        let mut response = source.open(descriptor.source_url, offset)?;
        match response.disposition {
            DownloadDisposition::Partial { start } if offset > 0 && start == offset => {}
            DownloadDisposition::Full if offset > 0 => {
                offset = 0;
                remove_if_present(&part_path)?;
            }
            DownloadDisposition::Full if offset == 0 => {}
            DownloadDisposition::Partial { start: 0 } if offset == 0 => {}
            _ => return Err(ModelManagerError::DownloadProtocolInvalid),
        }
        if response.total_bytes != descriptor.download_bytes {
            return Err(ModelManagerError::DownloadProtocolInvalid);
        }

        let metadata = ResumeMetadata {
            model_id: descriptor.id.to_owned(),
            source_revision: descriptor.source_revision.to_owned(),
            expected_bytes: descriptor.download_bytes,
            sha256: descriptor.sha256.to_owned(),
            etag: response.etag.take(),
            last_modified: response.last_modified.take(),
        };
        self.write_resume_metadata(descriptor, &metadata)?;

        let mut options = OpenOptions::new();
        options.create(true).write(true);
        if offset == 0 {
            options.truncate(true);
        } else {
            options.append(true);
        }
        let mut output = options
            .open(&part_path)
            .map_err(|_| ModelManagerError::FileOperationFailed)?;
        let mut buffer = [0_u8; COPY_BUFFER_BYTES];
        let mut downloaded = offset;
        progress(InstallProgress::Downloading {
            bytes_downloaded: downloaded,
            total_bytes: descriptor.download_bytes,
        });
        loop {
            if cancel.load(Ordering::Relaxed) {
                output
                    .sync_all()
                    .map_err(|_| ModelManagerError::FileOperationFailed)?;
                return Ok(InstallOutcome::Cancelled);
            }
            let read = match response.reader.read(&mut buffer) {
                Ok(read) => read,
                Err(_) => {
                    output
                        .sync_all()
                        .map_err(|_| ModelManagerError::FileOperationFailed)?;
                    return Err(ModelManagerError::DownloadInterrupted);
                }
            };
            if read == 0 {
                break;
            }
            downloaded = downloaded
                .checked_add(read as u64)
                .ok_or(ModelManagerError::DownloadProtocolInvalid)?;
            if downloaded > descriptor.download_bytes {
                self.clear_staging(descriptor)?;
                return Err(ModelManagerError::DownloadProtocolInvalid);
            }
            output
                .write_all(&buffer[..read])
                .map_err(|_| ModelManagerError::FileOperationFailed)?;
            progress(InstallProgress::Downloading {
                bytes_downloaded: downloaded,
                total_bytes: descriptor.download_bytes,
            });
        }
        output
            .sync_all()
            .map_err(|_| ModelManagerError::FileOperationFailed)?;
        drop(output);

        if downloaded != descriptor.download_bytes {
            return Err(ModelManagerError::DownloadInterrupted);
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(InstallOutcome::Cancelled);
        }
        progress(InstallProgress::Verifying {
            total_bytes: descriptor.download_bytes,
        });
        if verify_file(&part_path, descriptor).is_err() {
            self.clear_staging(descriptor)?;
            return Err(ModelManagerError::StagedModelInvalid);
        }
        fs::rename(&part_path, &final_path).map_err(|_| ModelManagerError::FileOperationFailed)?;
        remove_if_present(&self.metadata_path(descriptor))?;
        Ok(InstallOutcome::Installed)
    }

    #[cfg(test)]
    fn install_from(
        &self,
        descriptor: &ModelDescriptor,
        source: &dyn DownloadSource,
        cancel: &AtomicBool,
        available_disk_bytes: u64,
    ) -> Result<InstallOutcome, ModelManagerError> {
        self.install_from_with_progress(
            descriptor,
            source,
            cancel,
            available_disk_bytes,
            &mut |_| {},
        )
    }

    fn normalize_staging(&self, descriptor: &ModelDescriptor) -> Result<(), ModelManagerError> {
        let part_path = self.part_path(descriptor);
        let metadata_path = self.metadata_path(descriptor);
        let part_len = file_len_or_zero(&part_path)?;
        if part_len == 0 {
            remove_if_present(&part_path)?;
            remove_if_present(&metadata_path)?;
            return Ok(());
        }
        if part_len > descriptor.download_bytes {
            return self.clear_staging(descriptor);
        }
        let raw = match read_bounded_regular_file(&metadata_path, MAX_RESUME_METADATA_BYTES) {
            Ok(raw) => raw,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                return self.clear_staging(descriptor);
            }
            Err(_) => return Err(ModelManagerError::FileOperationFailed),
        };
        let metadata: ResumeMetadata = match serde_json::from_slice(&raw) {
            Ok(metadata) => metadata,
            Err(_) => return self.clear_staging(descriptor),
        };
        if !metadata.matches(descriptor) {
            return self.clear_staging(descriptor);
        }
        Ok(())
    }

    fn valid_staged_bytes(&self, descriptor: &ModelDescriptor) -> Result<u64, ModelManagerError> {
        let part_len = file_len_or_zero(&self.part_path(descriptor))?;
        if part_len == 0 || part_len > descriptor.download_bytes {
            return Ok(0);
        }
        let raw = match read_bounded_regular_file(
            &self.metadata_path(descriptor),
            MAX_RESUME_METADATA_BYTES,
        ) {
            Ok(raw) => raw,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                return Ok(0);
            }
            Err(_) => return Err(ModelManagerError::FileOperationFailed),
        };
        let metadata = serde_json::from_slice::<ResumeMetadata>(&raw).ok();
        Ok(metadata
            .filter(|metadata| metadata.matches(descriptor))
            .map_or(0, |_| part_len))
    }

    fn write_resume_metadata(
        &self,
        descriptor: &ModelDescriptor,
        metadata: &ResumeMetadata,
    ) -> Result<(), ModelManagerError> {
        let temporary = self.root.join(format!("{}.resume.tmp", descriptor.id));
        let final_path = self.metadata_path(descriptor);
        let bytes =
            serde_json::to_vec(metadata).map_err(|_| ModelManagerError::FileOperationFailed)?;
        {
            let mut file =
                File::create(&temporary).map_err(|_| ModelManagerError::FileOperationFailed)?;
            file.write_all(&bytes)
                .map_err(|_| ModelManagerError::FileOperationFailed)?;
            file.sync_all()
                .map_err(|_| ModelManagerError::FileOperationFailed)?;
        }
        remove_if_present(&final_path)?;
        fs::rename(temporary, final_path).map_err(|_| ModelManagerError::FileOperationFailed)
    }

    fn clear_staging(&self, descriptor: &ModelDescriptor) -> Result<(), ModelManagerError> {
        remove_if_present(&self.part_path(descriptor))?;
        remove_if_present(&self.metadata_path(descriptor))?;
        remove_if_present(&self.root.join(format!("{}.resume.tmp", descriptor.id)))
    }

    fn final_path(&self, descriptor: &ModelDescriptor) -> PathBuf {
        self.root.join(descriptor.file_name)
    }

    fn part_path(&self, descriptor: &ModelDescriptor) -> PathBuf {
        self.root.join(format!("{}.part", descriptor.id))
    }

    fn metadata_path(&self, descriptor: &ModelDescriptor) -> PathBuf {
        self.root.join(format!("{}.resume.json", descriptor.id))
    }

    fn model_paths(&self, descriptor: &ModelDescriptor) -> [PathBuf; 4] {
        [
            self.final_path(descriptor),
            self.part_path(descriptor),
            self.metadata_path(descriptor),
            self.root.join(format!("{}.resume.tmp", descriptor.id)),
        ]
    }
}

trait DownloadSource {
    fn open(&self, url: &str, offset: u64) -> Result<DownloadResponse, ModelManagerError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DownloadDisposition {
    Full,
    Partial { start: u64 },
}

struct DownloadResponse {
    disposition: DownloadDisposition,
    total_bytes: u64,
    etag: Option<String>,
    last_modified: Option<String>,
    reader: Box<dyn Read + Send>,
}

struct HttpDownloadSource {
    client: Client,
}

impl HttpDownloadSource {
    fn new() -> Result<Self, ModelManagerError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30 * 60))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|_| ModelManagerError::DownloadUnavailable)?;
        Ok(Self { client })
    }
}

impl DownloadSource for HttpDownloadSource {
    fn open(&self, url: &str, offset: u64) -> Result<DownloadResponse, ModelManagerError> {
        let mut request = self.client.get(url);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .map_err(|_| ModelManagerError::DownloadUnavailable)?;
        http_response(response, offset)
    }
}

fn http_response(response: Response, offset: u64) -> Result<DownloadResponse, ModelManagerError> {
    let status = response.status();
    let headers = response.headers();
    let (disposition, total_bytes) = if status == StatusCode::OK {
        let length = header_u64(headers.get(CONTENT_LENGTH))?;
        (DownloadDisposition::Full, length)
    } else if status == StatusCode::PARTIAL_CONTENT {
        let (start, total) = parse_content_range(
            headers
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .ok_or(ModelManagerError::DownloadProtocolInvalid)?,
        )?;
        if start != offset {
            return Err(ModelManagerError::DownloadProtocolInvalid);
        }
        (DownloadDisposition::Partial { start }, total)
    } else {
        return Err(ModelManagerError::DownloadUnavailable);
    };
    let etag = header_string(headers.get(ETAG));
    let last_modified = header_string(headers.get(LAST_MODIFIED));
    Ok(DownloadResponse {
        disposition,
        total_bytes,
        etag,
        last_modified,
        reader: Box::new(response),
    })
}

fn header_u64(value: Option<&reqwest::header::HeaderValue>) -> Result<u64, ModelManagerError> {
    value
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .ok_or(ModelManagerError::DownloadProtocolInvalid)
}

fn header_string(value: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    value
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 512)
        .map(str::to_owned)
}

fn parse_content_range(value: &str) -> Result<(u64, u64), ModelManagerError> {
    let range = value
        .strip_prefix("bytes ")
        .ok_or(ModelManagerError::DownloadProtocolInvalid)?;
    let (span, total) = range
        .split_once('/')
        .ok_or(ModelManagerError::DownloadProtocolInvalid)?;
    let (start, end) = span
        .split_once('-')
        .ok_or(ModelManagerError::DownloadProtocolInvalid)?;
    let start = start
        .parse::<u64>()
        .map_err(|_| ModelManagerError::DownloadProtocolInvalid)?;
    let end = end
        .parse::<u64>()
        .map_err(|_| ModelManagerError::DownloadProtocolInvalid)?;
    let total = total
        .parse::<u64>()
        .map_err(|_| ModelManagerError::DownloadProtocolInvalid)?;
    if start > end || end >= total {
        return Err(ModelManagerError::DownloadProtocolInvalid);
    }
    Ok((start, total))
}

fn compatibility_from(
    descriptor: &ModelDescriptor,
    available_disk_bytes: u64,
    required_disk_bytes: u64,
    available_memory_bytes: u64,
) -> Compatibility {
    Compatibility {
        available_disk_bytes,
        required_disk_bytes,
        available_memory_bytes,
        approximate_memory_bytes: descriptor.approximate_memory_bytes,
        disk_compatible: available_disk_bytes >= required_disk_bytes,
        memory_compatible: available_memory_bytes >= descriptor.approximate_memory_bytes,
    }
}

fn verify_file(path: &Path, descriptor: &ModelDescriptor) -> Result<(), ModelManagerError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| ModelManagerError::InstalledModelInvalid)?;
    if !metadata.file_type().is_file() || has_reparse_attribute(&metadata) {
        return Err(ModelManagerError::InstalledModelInvalid);
    }
    if metadata.len() != descriptor.download_bytes {
        return Err(ModelManagerError::InstalledModelInvalid);
    }
    let file = File::open(path).map_err(|_| ModelManagerError::InstalledModelInvalid)?;
    let mut reader = BufReader::with_capacity(COPY_BUFFER_BYTES, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| ModelManagerError::InstalledModelInvalid)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != descriptor.sha256 {
        return Err(ModelManagerError::InstalledModelInvalid);
    }
    Ok(())
}

fn read_bounded_regular_file(path: &Path, maximum_bytes: u64) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || has_reparse_attribute(&metadata)
        || metadata.len() > maximum_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bounded regular file required",
        ));
    }
    fs::read(path)
}

fn file_len_or_zero(path: &Path) -> Result<u64, ModelManagerError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(metadata.len()),
        Ok(_) => Err(ModelManagerError::UnsafeModelRoot),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(_) => Err(ModelManagerError::FileOperationFailed),
    }
}

fn remove_exact_file(path: &Path) -> Result<(), ModelManagerError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::remove_file(path).map_err(|_| ModelManagerError::FileOperationFailed)
        }
        Ok(_) => Err(ModelManagerError::UnsafeModelRoot),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ModelManagerError::FileOperationFailed),
    }
}

fn remove_if_present(path: &Path) -> Result<(), ModelManagerError> {
    remove_exact_file(path)
}

fn reject_reparse_root(path: &Path) -> Result<(), ModelManagerError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| ModelManagerError::ModelRootUnavailable)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || has_reparse_attribute(&metadata)
    {
        return Err(ModelManagerError::UnsafeModelRoot);
    }
    Ok(())
}

#[cfg(windows)]
fn has_reparse_attribute(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0000_0400 != 0
}

#[cfg(not(windows))]
fn has_reparse_attribute(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn available_memory_bytes() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: status is initialized with the documented structure size and is valid for the call.
    if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
        status.ullAvailPhys
    } else {
        0
    }
}

#[cfg(not(windows))]
const fn available_memory_bytes() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{Arc, Mutex},
    };

    use tempfile::TempDir;

    use super::*;

    const TEST_BYTES: &[u8] = b"bounded whisper model fixture";

    fn test_descriptor() -> ModelDescriptor {
        ModelDescriptor {
            id: "test-model",
            engine: "whisper",
            name: "Test model",
            source_url: "https://example.invalid/model.bin",
            source_revision: "revision",
            file_name: "model.bin",
            sha256: "8dd9ddc9edfaf6fff4cb9b049ab612b4ff4a7ee4f872a1b1e21d2f50c2aa8828",
            download_bytes: TEST_BYTES.len() as u64,
            disk_bytes: TEST_BYTES.len() as u64,
            languages: &["multilingual"],
            approximate_memory_bytes: 32,
            performance_class: "fast",
            backends: &["cpu", "vulkan"],
            license_spdx: "MIT",
            license_url: "https://example.invalid/license",
        }
    }

    struct FakeSource {
        bytes: Vec<u8>,
        offsets: Mutex<Vec<u64>>,
        ignore_range: bool,
    }

    impl FakeSource {
        fn new(bytes: &[u8]) -> Self {
            Self {
                bytes: bytes.to_vec(),
                offsets: Mutex::new(Vec::new()),
                ignore_range: false,
            }
        }
    }

    impl DownloadSource for FakeSource {
        fn open(&self, _url: &str, offset: u64) -> Result<DownloadResponse, ModelManagerError> {
            self.offsets.lock().expect("offset lock").push(offset);
            let actual_offset = if self.ignore_range { 0 } else { offset };
            Ok(DownloadResponse {
                disposition: if actual_offset == 0 {
                    DownloadDisposition::Full
                } else {
                    DownloadDisposition::Partial {
                        start: actual_offset,
                    }
                },
                total_bytes: self.bytes.len() as u64,
                etag: Some("fixture-etag".to_owned()),
                last_modified: None,
                reader: Box::new(Cursor::new(self.bytes[actual_offset as usize..].to_vec())),
            })
        }
    }

    fn manager() -> (TempDir, ModelManager) {
        let temporary = TempDir::new().expect("temporary directory");
        let manager = ModelManager::open_app_local(temporary.path()).expect("model manager");
        (temporary, manager)
    }

    fn write_partial(manager: &ModelManager, descriptor: &ModelDescriptor, length: usize) {
        fs::write(manager.part_path(descriptor), &TEST_BYTES[..length]).expect("partial file");
        manager
            .write_resume_metadata(
                descriptor,
                &ResumeMetadata {
                    model_id: descriptor.id.to_owned(),
                    source_revision: descriptor.source_revision.to_owned(),
                    expected_bytes: descriptor.download_bytes,
                    sha256: descriptor.sha256.to_owned(),
                    etag: Some("old-etag".to_owned()),
                    last_modified: None,
                },
            )
            .expect("resume metadata");
    }

    #[test]
    fn clean_install_is_verified_atomic_and_idempotent() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        let source = FakeSource::new(TEST_BYTES);
        let cancel = AtomicBool::new(false);
        assert_eq!(
            manager.install_from(&descriptor, &source, &cancel, u64::MAX),
            Ok(InstallOutcome::Installed)
        );
        assert_eq!(
            fs::read(manager.final_path(&descriptor)).expect("installed"),
            TEST_BYTES
        );
        assert!(!manager.part_path(&descriptor).exists());
        assert!(!manager.metadata_path(&descriptor).exists());
        assert_eq!(
            manager.install_from(&descriptor, &source, &cancel, u64::MAX),
            Ok(InstallOutcome::AlreadyInstalled)
        );
        assert_eq!(*source.offsets.lock().expect("offset lock"), vec![0]);
    }

    #[test]
    fn valid_partial_resumes_and_ignored_range_restarts() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        write_partial(&manager, &descriptor, 7);
        let source = FakeSource::new(TEST_BYTES);
        manager
            .install_from(&descriptor, &source, &AtomicBool::new(false), u64::MAX)
            .expect("resume install");
        assert_eq!(*source.offsets.lock().expect("offset lock"), vec![7]);

        fs::remove_file(manager.final_path(&descriptor)).expect("remove fixture install");
        write_partial(&manager, &descriptor, 5);
        let source = FakeSource {
            ignore_range: true,
            ..FakeSource::new(TEST_BYTES)
        };
        manager
            .install_from(&descriptor, &source, &AtomicBool::new(false), u64::MAX)
            .expect("restart install");
        assert_eq!(
            fs::read(manager.final_path(&descriptor)).expect("installed"),
            TEST_BYTES
        );
        assert_eq!(*source.offsets.lock().expect("offset lock"), vec![5]);
    }

    struct CancellingReader {
        bytes: Cursor<Vec<u8>>,
        cancel: Arc<AtomicBool>,
        reads: usize,
    }

    impl Read for CancellingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let limit = buffer.len().min(8);
            let read = self.bytes.read(&mut buffer[..limit])?;
            self.reads += 1;
            if self.reads == 1 {
                self.cancel.store(true, Ordering::Relaxed);
            }
            Ok(read)
        }
    }

    struct CancellingSource(Arc<AtomicBool>);

    impl DownloadSource for CancellingSource {
        fn open(&self, _url: &str, _offset: u64) -> Result<DownloadResponse, ModelManagerError> {
            Ok(DownloadResponse {
                disposition: DownloadDisposition::Full,
                total_bytes: TEST_BYTES.len() as u64,
                etag: None,
                last_modified: None,
                reader: Box::new(CancellingReader {
                    bytes: Cursor::new(TEST_BYTES.to_vec()),
                    cancel: Arc::clone(&self.0),
                    reads: 0,
                }),
            })
        }
    }

    #[test]
    fn cancellation_leaves_resumable_state_then_resume_completes() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        let cancel = Arc::new(AtomicBool::new(false));
        let outcome = manager
            .install_from(
                &descriptor,
                &CancellingSource(Arc::clone(&cancel)),
                &cancel,
                u64::MAX,
            )
            .expect("cancelled install");
        assert_eq!(outcome, InstallOutcome::Cancelled);
        assert_eq!(
            file_len_or_zero(&manager.part_path(&descriptor)).expect("part length"),
            8
        );
        cancel.store(false, Ordering::Relaxed);
        manager
            .install_from(&descriptor, &FakeSource::new(TEST_BYTES), &cancel, u64::MAX)
            .expect("resumed install");
        assert_eq!(
            fs::read(manager.final_path(&descriptor)).expect("installed"),
            TEST_BYTES
        );
    }

    #[test]
    fn complete_verified_staging_installs_without_another_download() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        write_partial(&manager, &descriptor, TEST_BYTES.len());
        let source = FakeSource::new(TEST_BYTES);
        assert_eq!(
            manager.install_from(&descriptor, &source, &AtomicBool::new(false), u64::MAX),
            Ok(InstallOutcome::Installed)
        );
        assert!(source.offsets.lock().expect("offset lock").is_empty());
        assert_eq!(
            fs::read(manager.final_path(&descriptor)).expect("installed"),
            TEST_BYTES
        );
    }

    #[test]
    fn corrupt_staging_and_install_are_removed_before_recovery() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        fs::write(manager.part_path(&descriptor), b"wrong").expect("bad part");
        fs::write(manager.metadata_path(&descriptor), b"not json").expect("bad metadata");
        manager
            .install_from(
                &descriptor,
                &FakeSource::new(TEST_BYTES),
                &AtomicBool::new(false),
                u64::MAX,
            )
            .expect("recover invalid staging");
        fs::write(
            manager.final_path(&descriptor),
            vec![0_u8; TEST_BYTES.len()],
        )
        .expect("corrupt install");
        manager
            .install_from(
                &descriptor,
                &FakeSource::new(TEST_BYTES),
                &AtomicBool::new(false),
                u64::MAX,
            )
            .expect("recover corrupt install");
        assert_eq!(
            fs::read(manager.final_path(&descriptor)).expect("installed"),
            TEST_BYTES
        );
    }

    #[test]
    fn oversized_resume_metadata_is_never_read_unbounded() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        fs::write(manager.part_path(&descriptor), &TEST_BYTES[..4]).expect("partial stage");
        fs::write(
            manager.metadata_path(&descriptor),
            vec![b'x'; MAX_RESUME_METADATA_BYTES as usize + 1],
        )
        .expect("oversized metadata");
        manager
            .install_from(
                &descriptor,
                &FakeSource::new(TEST_BYTES),
                &AtomicBool::new(false),
                u64::MAX,
            )
            .expect("bounded metadata recovery");
        assert_eq!(
            fs::read(manager.final_path(&descriptor)).expect("installed"),
            TEST_BYTES
        );
    }

    #[test]
    fn insufficient_disk_blocks_before_source_open() {
        let (_temporary, manager) = manager();
        let descriptor = test_descriptor();
        let source = FakeSource::new(TEST_BYTES);
        assert_eq!(
            manager.install_from(&descriptor, &source, &AtomicBool::new(false), 0),
            Err(ModelManagerError::InsufficientDisk)
        );
        assert!(source.offsets.lock().expect("offset lock").is_empty());
    }

    #[test]
    fn selection_requires_verified_install_and_deletion_is_catalog_bounded() {
        let (_temporary, mut manager) = manager();
        assert_eq!(
            manager.select(catalog::TINY.id),
            Err(ModelManagerError::ModelNotInstalled)
        );
        fs::write(manager.final_path(&catalog::TINY), vec![0_u8; 10]).expect("invalid install");
        assert_eq!(
            manager.select(catalog::TINY.id),
            Err(ModelManagerError::InstalledModelInvalid)
        );
        assert_eq!(
            manager.delete("../outside"),
            Err(ModelManagerError::UnknownModel)
        );
        assert!(manager.root().join("..").exists());
    }

    #[test]
    fn verified_selection_blocks_deletion_until_cleared() {
        let (_temporary, mut manager) = manager();
        let descriptor = Box::leak(Box::new(test_descriptor()));
        manager
            .install_from(
                descriptor,
                &FakeSource::new(TEST_BYTES),
                &AtomicBool::new(false),
                u64::MAX,
            )
            .expect("fixture install");
        manager.select_descriptor(descriptor).expect("selection");
        assert_eq!(manager.selected_model_id(), Some(descriptor.id));
        assert_eq!(
            manager.delete_descriptor(descriptor),
            Err(ModelManagerError::SelectedModelCannotBeDeleted)
        );
        manager.clear_selection();
        assert_eq!(
            manager.delete_descriptor(descriptor),
            Ok(TEST_BYTES.len() as u64)
        );
        assert!(!manager.final_path(descriptor).exists());
    }

    #[test]
    fn compatibility_distinguishes_disk_block_from_memory_warning() {
        let descriptor = test_descriptor();
        let compatibility = compatibility_from(&descriptor, 99, 100, 31);
        assert!(!compatibility.disk_compatible);
        assert!(!compatibility.memory_compatible);
        let compatibility = compatibility_from(&descriptor, 100, 100, 32);
        assert!(compatibility.disk_compatible);
        assert!(compatibility.memory_compatible);
    }

    #[test]
    fn content_range_parser_rejects_ambiguous_or_invalid_ranges() {
        assert_eq!(parse_content_range("bytes 7-9/10"), Ok((7, 10)));
        assert_eq!(
            parse_content_range("bytes 9-7/10"),
            Err(ModelManagerError::DownloadProtocolInvalid)
        );
        assert_eq!(
            parse_content_range("bytes */10"),
            Err(ModelManagerError::DownloadProtocolInvalid)
        );
        assert_eq!(
            parse_content_range("items 0-1/2"),
            Err(ModelManagerError::DownloadProtocolInvalid)
        );
    }

    #[test]
    #[ignore = "requires the external P3-004 Tiny and Base model files"]
    fn exact_external_catalog_files_match_pinned_hashes() {
        let tiny = std::env::var_os("KOKOROKOE_P3_010_TINY_MODEL").expect("Tiny model path");
        let base = std::env::var_os("KOKOROKOE_P3_010_BASE_MODEL").expect("Base model path");
        verify_file(Path::new(&tiny), &catalog::TINY).expect("Tiny catalog hash");
        verify_file(Path::new(&base), &catalog::BASE).expect("Base catalog hash");
    }

    #[test]
    #[ignore = "requires the external P5-015 Large-v3 Turbo Q5_0 model file"]
    fn exact_external_large_v3_turbo_file_matches_pinned_hash() {
        let turbo =
            std::env::var_os("KOKOROKOE_P5_015_TURBO_MODEL").expect("Large-v3 Turbo model path");
        verify_file(Path::new(&turbo), &catalog::LARGE_V3_TURBO_Q5_0)
            .expect("Large-v3 Turbo catalog hash");
    }
}
