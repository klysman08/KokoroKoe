use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use super::AppError;

const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelBackend {
    Cpu,
    Vulkan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PerformanceClass {
    Fast,
    Balanced,
    Accurate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelInstallationStatus {
    NotInstalled,
    Downloading,
    Installed,
    Failed,
    Incompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelDownloadStatus {
    Queued,
    Downloading,
    Paused,
    Verifying,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub(crate) struct RequestId(Uuid);

impl RequestId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub(crate) fn as_uuid(&self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelDescriptor {
    pub(crate) id: String,
    pub(crate) engine: String,
    pub(crate) name: String,
    pub(crate) source_url: String,
    pub(crate) source_revision: String,
    pub(crate) file_name: String,
    pub(crate) sha256: String,
    pub(crate) download_bytes: u64,
    pub(crate) disk_bytes: u64,
    pub(crate) languages: Vec<String>,
    pub(crate) approximate_memory_bytes: u64,
    pub(crate) performance_class: PerformanceClass,
    pub(crate) backends: Vec<ModelBackend>,
    pub(crate) license_spdx: String,
    pub(crate) license_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawModelDescriptor {
    id: String,
    engine: String,
    name: String,
    source_url: String,
    source_revision: String,
    file_name: String,
    sha256: String,
    download_bytes: u64,
    disk_bytes: u64,
    languages: Vec<String>,
    approximate_memory_bytes: u64,
    performance_class: PerformanceClass,
    backends: Vec<ModelBackend>,
    license_spdx: String,
    license_url: String,
}

impl<'de> Deserialize<'de> for ModelDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawModelDescriptor::deserialize(deserializer)?;
        let descriptor = Self {
            id: raw.id,
            engine: raw.engine,
            name: raw.name,
            source_url: raw.source_url,
            source_revision: raw.source_revision,
            file_name: raw.file_name,
            sha256: raw.sha256,
            download_bytes: raw.download_bytes,
            disk_bytes: raw.disk_bytes,
            languages: raw.languages,
            approximate_memory_bytes: raw.approximate_memory_bytes,
            performance_class: raw.performance_class,
            backends: raw.backends,
            license_spdx: raw.license_spdx,
            license_url: raw.license_url,
        };
        descriptor.validate().map_err(D::Error::custom)?;
        Ok(descriptor)
    }
}

impl ModelDescriptor {
    fn validate(&self) -> Result<(), &'static str> {
        if !bounded(&self.id, 1, 128)
            || self.engine != "whisper"
            || !bounded(&self.name, 1, 128)
            || !bounded(&self.source_url, 1, 2048)
            || !self.source_url.starts_with("https://")
            || !bounded(&self.source_revision, 1, 128)
            || !bounded(&self.file_name, 1, 128)
            || self.file_name.contains(['/', '\\'])
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.download_bytes == 0
            || self.download_bytes > JSON_SAFE_INTEGER_MAX
            || self.disk_bytes != self.download_bytes
            || self.approximate_memory_bytes == 0
            || self.approximate_memory_bytes > JSON_SAFE_INTEGER_MAX
            || self.languages.is_empty()
            || self.languages.len() > 16
            || self.languages.iter().any(|value| !bounded(value, 1, 64))
            || has_duplicates(&self.languages)
            || self.backends.is_empty()
            || self.backends.len() > 2
            || has_duplicates(&self.backends)
            || !bounded(&self.license_spdx, 1, 64)
            || !bounded(&self.license_url, 1, 2048)
            || !self.license_url.starts_with("https://")
        {
            return Err("The model descriptor is outside its transport contract.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelCompatibility {
    pub(crate) available_disk_bytes: u64,
    pub(crate) required_disk_bytes: u64,
    pub(crate) available_memory_bytes: u64,
    pub(crate) approximate_memory_bytes: u64,
    pub(crate) disk_compatible: bool,
    pub(crate) memory_compatible: bool,
}

impl ModelCompatibility {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if [
            self.available_disk_bytes,
            self.required_disk_bytes,
            self.available_memory_bytes,
            self.approximate_memory_bytes,
        ]
        .into_iter()
        .any(|value| value > JSON_SAFE_INTEGER_MAX)
            || self.disk_compatible != (self.available_disk_bytes >= self.required_disk_bytes)
            || self.memory_compatible
                != (self.available_memory_bytes >= self.approximate_memory_bytes)
        {
            return Err("The model compatibility values are inconsistent.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelDownloadJob {
    pub(crate) request_id: RequestId,
    pub(crate) model_id: String,
    pub(crate) status: ModelDownloadStatus,
    pub(crate) bytes_downloaded: u64,
    pub(crate) total_bytes: u64,
    pub(crate) resumable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_modified: Option<String>,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<AppError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawModelDownloadJob {
    request_id: RequestId,
    model_id: String,
    status: ModelDownloadStatus,
    bytes_downloaded: u64,
    total_bytes: u64,
    resumable: bool,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    etag: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    last_modified: Option<String>,
    started_at: String,
    updated_at: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    error: Option<AppError>,
}

impl<'de> Deserialize<'de> for ModelDownloadJob {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawModelDownloadJob::deserialize(deserializer)?;
        let job = Self {
            request_id: raw.request_id,
            model_id: raw.model_id,
            status: raw.status,
            bytes_downloaded: raw.bytes_downloaded,
            total_bytes: raw.total_bytes,
            resumable: raw.resumable,
            etag: raw.etag,
            last_modified: raw.last_modified,
            started_at: raw.started_at,
            updated_at: raw.updated_at,
            error: raw.error,
        };
        job.validate().map_err(D::Error::custom)?;
        Ok(job)
    }
}

impl ModelDownloadJob {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !bounded(&self.model_id, 1, 128)
            || self.total_bytes == 0
            || self.total_bytes > JSON_SAFE_INTEGER_MAX
            || self.bytes_downloaded > self.total_bytes
            || !valid_rfc3339(&self.started_at)
            || !valid_rfc3339(&self.updated_at)
            || self
                .etag
                .as_deref()
                .is_some_and(|value| !bounded(value, 1, 512))
            || self
                .last_modified
                .as_deref()
                .is_some_and(|value| !bounded(value, 1, 128))
            || (self.status == ModelDownloadStatus::Completed
                && self.bytes_downloaded != self.total_bytes)
            || (self.status == ModelDownloadStatus::Failed && self.error.is_none())
            || (self.status != ModelDownloadStatus::Failed && self.error.is_some())
        {
            return Err("The model download job is outside its transport contract.");
        }
        Ok(())
    }

    pub(crate) fn is_interrupted_running(&self) -> bool {
        matches!(
            self.status,
            ModelDownloadStatus::Queued
                | ModelDownloadStatus::Downloading
                | ModelDownloadStatus::Verifying
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelInstallation {
    pub(crate) descriptor: ModelDescriptor,
    pub(crate) status: ModelInstallationStatus,
    pub(crate) installed_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) installed_at: Option<String>,
    pub(crate) selected_as_default: bool,
    pub(crate) available_backends: Vec<ModelBackend>,
    pub(crate) compatibility: ModelCompatibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) download_job: Option<ModelDownloadJob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_error: Option<AppError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawModelInstallation {
    descriptor: ModelDescriptor,
    status: ModelInstallationStatus,
    installed_bytes: u64,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    installed_at: Option<String>,
    selected_as_default: bool,
    available_backends: Vec<ModelBackend>,
    compatibility: ModelCompatibility,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    download_job: Option<ModelDownloadJob>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    last_error: Option<AppError>,
}

impl<'de> Deserialize<'de> for ModelInstallation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawModelInstallation::deserialize(deserializer)?;
        let installation = Self {
            descriptor: raw.descriptor,
            status: raw.status,
            installed_bytes: raw.installed_bytes,
            installed_at: raw.installed_at,
            selected_as_default: raw.selected_as_default,
            available_backends: raw.available_backends,
            compatibility: raw.compatibility,
            download_job: raw.download_job,
            last_error: raw.last_error,
        };
        installation.validate().map_err(D::Error::custom)?;
        Ok(installation)
    }
}

impl ModelInstallation {
    fn validate(&self) -> Result<(), &'static str> {
        self.compatibility.validate()?;
        if self.installed_bytes > self.descriptor.disk_bytes
            || self.compatibility.approximate_memory_bytes
                != self.descriptor.approximate_memory_bytes
            || self
                .installed_at
                .as_deref()
                .is_some_and(|value| !valid_rfc3339(value))
            || self.available_backends.is_empty()
            || self.available_backends.len() > 2
            || has_duplicates(&self.available_backends)
            || self
                .available_backends
                .iter()
                .any(|backend| !self.descriptor.backends.contains(backend))
            || self
                .download_job
                .as_ref()
                .is_some_and(|job| job.model_id != self.descriptor.id)
            || (self.status == ModelInstallationStatus::Installed
                && (self.installed_bytes != self.descriptor.disk_bytes
                    || self.installed_at.is_none()))
            || (self.status != ModelInstallationStatus::Installed && self.installed_bytes != 0)
            || (self.status == ModelInstallationStatus::Failed && self.last_error.is_none())
        {
            return Err("The model installation is outside its transport contract.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelDownloadProgress {
    pub(crate) job: ModelDownloadJob,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bytes_per_second: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventEnvelope<T> {
    pub(crate) schema_version: u8,
    pub(crate) event_id: Uuid,
    pub(crate) emitted_at: String,
    pub(crate) request_id: RequestId,
    pub(crate) payload: T,
}

impl EventEnvelope<ModelDownloadProgress> {
    pub(crate) fn new(
        job: ModelDownloadJob,
        bytes_per_second: Option<u64>,
    ) -> Result<Self, AppError> {
        let request_id = job.request_id;
        let event = Self {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            emitted_at: now_rfc3339()?,
            request_id,
            payload: ModelDownloadProgress {
                job,
                bytes_per_second,
            },
        };
        event.validate().map_err(AppError::model_operation_failed)?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || !valid_rfc3339(&self.emitted_at)
            || self.request_id != self.payload.job.request_id
            || self
                .payload
                .bytes_per_second
                .is_some_and(|value| value > JSON_SAFE_INTEGER_MAX)
        {
            return Err("The model progress event is outside its transport contract.");
        }
        self.payload.job.validate()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelContractFixture {
    pub(crate) installation: ModelInstallation,
    pub(crate) event: EventEnvelope<ModelDownloadProgress>,
}

#[cfg(test)]
impl ModelContractFixture {
    fn validate(&self) -> Result<(), &'static str> {
        self.installation.validate()?;
        self.event.validate()?;
        if self.installation.download_job.as_ref() != Some(&self.event.payload.job) {
            return Err("The fixture job and progress event must match exactly.");
        }
        Ok(())
    }
}

pub(crate) fn now_rfc3339() -> Result<String, AppError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| AppError::model_operation_failed("The UTC timestamp could not be formatted."))
}

fn valid_rfc3339(value: &str) -> bool {
    bounded(value, 20, 64) && OffsetDateTime::parse(value, &Rfc3339).is_ok()
}

fn bounded(value: &str, minimum: usize, maximum: usize) -> bool {
    let length = value.encode_utf16().count();
    (minimum..=maximum).contains(&length) && !value.chars().any(char::is_control)
}

fn has_duplicates<T: Eq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[index + 1..].contains(value))
}

fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{ModelContractFixture, ModelDownloadStatus};

    #[test]
    fn model_contract_matches_the_shared_golden_fixture() {
        let fixture = include_str!("../../../fixtures/contracts/model-management-v1.json");
        let parsed: ModelContractFixture =
            serde_json::from_str(fixture).expect("model fixture should be valid");
        parsed.validate().expect("model fixture invariants");
        let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    }

    #[test]
    fn model_contract_rejects_unknown_nullable_and_inconsistent_values() {
        let fixture = include_str!("../../../fixtures/contracts/model-management-v1.json");
        let base: serde_json::Value = serde_json::from_str(fixture).unwrap();

        let mut unknown = base.clone();
        unknown["installation"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ModelContractFixture>(unknown).is_err());

        let mut nullable = base.clone();
        nullable["installation"]["installedAt"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ModelContractFixture>(nullable).is_err());

        let mut oversized = base.clone();
        oversized["event"]["payload"]["job"]["bytesDownloaded"] =
            serde_json::json!(9_007_199_254_740_992_u64);
        assert!(serde_json::from_value::<ModelContractFixture>(oversized).is_err());

        let mut mismatch = base.clone();
        mismatch["event"]["requestId"] = serde_json::json!("00000000-0000-4000-8000-000000000099");
        let parsed: ModelContractFixture = serde_json::from_value(mismatch).unwrap();
        assert!(parsed.validate().is_err());

        let mut casing = base;
        casing["installation"]["downloadJob"]["status"] = serde_json::json!("Downloading");
        assert!(serde_json::from_value::<ModelContractFixture>(casing).is_err());

        assert_ne!(
            ModelDownloadStatus::Downloading,
            ModelDownloadStatus::Paused
        );
    }
}
