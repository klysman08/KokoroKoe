use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    domain::{
        AppError, AppSettings, EventEnvelope, ModelBackend, ModelCompatibility,
        ModelDescriptor as TransportDescriptor, ModelDownloadJob, ModelDownloadStatus,
        ModelInstallation, ModelInstallationStatus, PerformanceClass, RequestId, now_rfc3339,
    },
    persistence::SettingsService,
};

use super::{
    ModelDescriptor, VerifiedModelArtifact, catalog,
    installer::{Compatibility, InstallOutcome, InstallProgress, ModelManager, ModelManagerError},
};

struct ActiveDownload {
    request_id: RequestId,
    model_id: String,
    cancel: Arc<AtomicBool>,
}

trait ModelStorageBackend: Send {
    fn compatibility(&self, model_id: &str) -> Result<Compatibility, ModelManagerError>;
    fn is_installed(&self, model_id: &str) -> Result<bool, ModelManagerError>;
    fn resumable_bytes(&self, model_id: &str) -> Result<u64, ModelManagerError>;
    fn install(
        &mut self,
        model_id: &str,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(InstallProgress),
    ) -> Result<InstallOutcome, ModelManagerError>;
    fn select(&mut self, model_id: &str) -> Result<(), ModelManagerError>;
    fn delete(&mut self, model_id: &str) -> Result<u64, ModelManagerError>;
    #[cfg_attr(not(test), allow(dead_code))]
    fn verified_model_path(&self, model_id: &str) -> Result<PathBuf, ModelManagerError>;
}

impl ModelStorageBackend for ModelManager {
    fn compatibility(&self, model_id: &str) -> Result<Compatibility, ModelManagerError> {
        self.compatibility(model_id)
    }

    fn is_installed(&self, model_id: &str) -> Result<bool, ModelManagerError> {
        self.is_installed(model_id)
    }

    fn resumable_bytes(&self, model_id: &str) -> Result<u64, ModelManagerError> {
        self.resumable_bytes(model_id)
    }

    fn install(
        &mut self,
        model_id: &str,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(InstallProgress),
    ) -> Result<InstallOutcome, ModelManagerError> {
        self.install_with_progress(model_id, cancel, progress)
    }

    fn select(&mut self, model_id: &str) -> Result<(), ModelManagerError> {
        self.select(model_id)
    }

    fn delete(&mut self, model_id: &str) -> Result<u64, ModelManagerError> {
        self.delete(model_id)
    }

    fn verified_model_path(&self, model_id: &str) -> Result<PathBuf, ModelManagerError> {
        self.verified_model_path(model_id)
    }
}

#[derive(Clone)]
pub(crate) struct ModelService {
    manager: Arc<Mutex<Box<dyn ModelStorageBackend>>>,
    settings: SettingsService,
    jobs: Arc<Mutex<HashMap<RequestId, ModelDownloadJob>>>,
    active: Arc<Mutex<Option<ActiveDownload>>>,
}

impl ModelService {
    pub(crate) fn open(
        app_local_data_directory: &Path,
        settings: SettingsService,
    ) -> Result<Self, AppError> {
        let manager = ModelManager::open_app_local(app_local_data_directory).map_err(map_error)?;
        Self::from_backend(settings, Box::new(manager))
    }

    fn from_backend(
        settings: SettingsService,
        manager: Box<dyn ModelStorageBackend>,
    ) -> Result<Self, AppError> {
        let stored_jobs = settings.load_model_jobs()?;
        let service = Self {
            manager: Arc::new(Mutex::new(manager)),
            settings,
            jobs: Arc::new(Mutex::new(
                stored_jobs
                    .into_iter()
                    .map(|job| (job.request_id, job))
                    .collect(),
            )),
            active: Arc::new(Mutex::new(None)),
        };
        service.recover_interrupted_jobs()?;
        Ok(service)
    }

    pub(crate) fn list(&self) -> Result<Vec<ModelInstallation>, AppError> {
        let settings = self.settings.get_settings()?;
        catalog::ALL
            .iter()
            .map(|descriptor| self.installation(descriptor, &settings))
            .collect()
    }

    pub(crate) fn queue_download(
        &self,
        model_id: &str,
        request_id: RequestId,
    ) -> Result<ModelDownloadJob, AppError> {
        let descriptor =
            catalog::find(model_id).ok_or_else(|| AppError::model_error("model_unknown"))?;
        let mut active = self.active.lock().map_err(lock_error)?;
        if self
            .jobs
            .lock()
            .map_err(lock_error)?
            .contains_key(&request_id)
        {
            return Err(AppError::model_error("model_request_id_conflict"));
        }
        if active.is_some() {
            return Err(AppError::model_error("model_download_in_progress"));
        }
        if self
            .manager
            .lock()
            .map_err(lock_error)?
            .is_installed(model_id)
            .map_err(map_error)?
        {
            return Err(AppError::model_error("model_already_installed"));
        }
        let resumable_bytes = self
            .manager
            .lock()
            .map_err(lock_error)?
            .resumable_bytes(model_id)
            .map_err(map_error)?;
        let timestamp = now_rfc3339()?;
        let job = ModelDownloadJob {
            request_id,
            model_id: descriptor.id.to_owned(),
            status: ModelDownloadStatus::Queued,
            bytes_downloaded: resumable_bytes,
            total_bytes: descriptor.download_bytes,
            resumable: resumable_bytes > 0,
            etag: None,
            last_modified: None,
            started_at: timestamp.clone(),
            updated_at: timestamp,
            error: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        *active = Some(ActiveDownload {
            request_id: job.request_id,
            model_id: job.model_id.clone(),
            cancel,
        });
        if let Err(error) = self.store_job(job.clone()) {
            *active = None;
            return Err(error);
        }
        Ok(job)
    }

    pub(crate) fn queue_resume(
        &self,
        model_id: &str,
        request_id: RequestId,
    ) -> Result<ModelDownloadJob, AppError> {
        if catalog::find(model_id).is_none() {
            return Err(AppError::model_error("model_unknown"));
        }
        if self
            .manager
            .lock()
            .map_err(lock_error)?
            .resumable_bytes(model_id)
            .map_err(map_error)?
            == 0
        {
            return Err(AppError::model_error("model_download_not_resumable"));
        }
        self.queue_download(model_id, request_id)
    }

    pub(crate) fn run_download(
        &self,
        request_id: RequestId,
        mut emit: impl FnMut(EventEnvelope<crate::domain::ModelDownloadProgress>) + Send,
    ) -> Result<(), AppError> {
        let (model_id, cancel) = {
            let active = self.active.lock().map_err(lock_error)?;
            let active = active
                .as_ref()
                .filter(|active| active.request_id == request_id)
                .ok_or_else(|| AppError::model_error("model_download_not_found"))?;
            (active.model_id.clone(), Arc::clone(&active.cancel))
        };
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut last_bytes = 0_u64;
        let mut last_time = Instant::now();
        let result =
            self.manager
                .lock()
                .map_err(lock_error)?
                .install(&model_id, &cancel, &mut |progress| {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let now = Instant::now();
                    let force = matches!(progress, InstallProgress::Verifying { .. });
                    if !force && now.duration_since(last_emit) < Duration::from_millis(200) {
                        return;
                    }
                    let (status, bytes_downloaded) = match progress {
                        InstallProgress::Downloading {
                            bytes_downloaded,
                            total_bytes: _,
                        } => (ModelDownloadStatus::Downloading, bytes_downloaded),
                        InstallProgress::Verifying { total_bytes } => {
                            (ModelDownloadStatus::Verifying, total_bytes)
                        }
                    };
                    let elapsed = now.duration_since(last_time).as_secs_f64();
                    let speed = (elapsed > 0.0).then(|| {
                        ((bytes_downloaded.saturating_sub(last_bytes)) as f64 / elapsed) as u64
                    });
                    if let Ok(job) = self.transition_job(request_id, status, bytes_downloaded, None)
                    {
                        if let Ok(event) = EventEnvelope::new(job, speed) {
                            emit(event);
                        }
                    }
                    last_emit = now;
                    last_bytes = bytes_downloaded;
                    last_time = now;
                });

        let terminal_result = (|| -> Result<ModelDownloadJob, AppError> {
            Ok(match result {
                Ok(InstallOutcome::Installed | InstallOutcome::AlreadyInstalled) => {
                    let installed_at = now_rfc3339()?;
                    self.settings
                        .record_model_installed(&model_id, &installed_at)?;
                    self.transition_job(
                        request_id,
                        ModelDownloadStatus::Completed,
                        catalog::find(&model_id)
                            .expect("queued catalog model")
                            .download_bytes,
                        None,
                    )?
                }
                Ok(InstallOutcome::Cancelled) => self.transition_job(
                    request_id,
                    ModelDownloadStatus::Cancelled,
                    self.manager
                        .lock()
                        .map_err(lock_error)?
                        .resumable_bytes(&model_id)
                        .map_err(map_error)?,
                    None,
                )?,
                Err(error) => {
                    let app_error = map_error(error);
                    let bytes = self
                        .manager
                        .lock()
                        .map_err(lock_error)?
                        .resumable_bytes(&model_id)
                        .unwrap_or(0);
                    self.transition_job(
                        request_id,
                        ModelDownloadStatus::Failed,
                        bytes,
                        Some(app_error),
                    )?
                }
            })
        })();
        let mut active = self.active.lock().map_err(lock_error)?;
        if active
            .as_ref()
            .is_some_and(|value| value.request_id == request_id)
        {
            *active = None;
        }
        drop(active);
        let terminal = terminal_result?;
        if let Ok(event) = EventEnvelope::new(terminal, None) {
            emit(event);
        }
        Ok(())
    }

    pub(crate) fn cancel(&self, request_id: RequestId) -> Result<ModelDownloadJob, AppError> {
        let active = self.active.lock().map_err(lock_error)?;
        let active = active
            .as_ref()
            .filter(|value| value.request_id == request_id)
            .ok_or_else(|| AppError::model_error("model_download_not_found"))?;
        active.cancel.store(true, Ordering::Relaxed);
        let bytes = self
            .manager
            .try_lock()
            .ok()
            .and_then(|manager| manager.resumable_bytes(&active.model_id).ok())
            .unwrap_or_else(|| {
                self.jobs
                    .lock()
                    .ok()
                    .and_then(|jobs| jobs.get(&request_id).map(|job| job.bytes_downloaded))
                    .unwrap_or(0)
            });
        self.transition_job(request_id, ModelDownloadStatus::Cancelled, bytes, None)
    }

    pub(crate) fn delete(&self, model_id: &str) -> Result<ModelInstallation, AppError> {
        if catalog::find(model_id).is_none() {
            return Err(AppError::model_error("model_unknown"));
        }
        if self
            .active
            .lock()
            .map_err(lock_error)?
            .as_ref()
            .is_some_and(|active| active.model_id == model_id)
        {
            return Err(AppError::model_error("model_active_job_cannot_be_deleted"));
        }
        let settings = self.settings.get_settings()?;
        if settings.default_transcription_model_id() == model_id {
            return Err(AppError::model_error("selected_model_cannot_be_deleted"));
        }
        self.manager
            .lock()
            .map_err(lock_error)?
            .delete(model_id)
            .map_err(map_error)?;
        self.settings.remove_model_installation(model_id)?;
        self.settings.remove_model_jobs(model_id)?;
        self.jobs
            .lock()
            .map_err(lock_error)?
            .retain(|_, job| job.model_id != model_id);
        self.installation(catalog::find(model_id).expect("catalog checked"), &settings)
    }

    pub(crate) fn set_default(
        &self,
        model_id: &str,
        expected_revision: u64,
    ) -> Result<AppSettings, AppError> {
        let mut manager = self.manager.lock().map_err(lock_error)?;
        if !manager.is_installed(model_id).map_err(map_error)? {
            return Err(AppError::model_error("model_not_installed"));
        }
        let settings = self
            .settings
            .set_default_transcription_model(expected_revision, model_id.to_owned())?;
        manager.select(model_id).map_err(map_error)?;
        Ok(settings)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resolve_default_for_transcription(
        &self,
    ) -> Result<VerifiedModelArtifact, AppError> {
        let model_id = self
            .settings
            .get_settings()?
            .default_transcription_model_id()
            .to_owned();
        self.resolve_for_transcription(&model_id)
    }

    pub(crate) fn resolve_for_transcription(
        &self,
        model_id: &str,
    ) -> Result<VerifiedModelArtifact, AppError> {
        if catalog::find(model_id).is_none() {
            return Err(AppError::model_error("model_unknown"));
        }
        let path = self
            .manager
            .lock()
            .map_err(lock_error)?
            .verified_model_path(model_id)
            .map_err(map_error)?;
        Ok(VerifiedModelArtifact {
            model_id: model_id.to_owned(),
            path,
        })
    }

    fn installation(
        &self,
        descriptor: &ModelDescriptor,
        settings: &AppSettings,
    ) -> Result<ModelInstallation, AppError> {
        let manager = self.manager.lock().map_err(lock_error)?;
        let installed = manager.is_installed(descriptor.id).map_err(map_error)?;
        let compatibility = manager.compatibility(descriptor.id).map_err(map_error)?;
        drop(manager);
        let installed_at = if installed {
            match self.settings.model_installed_at(descriptor.id)? {
                Some(value) => Some(value),
                None => {
                    let value = now_rfc3339()?;
                    self.settings
                        .record_model_installed(descriptor.id, &value)?;
                    Some(value)
                }
            }
        } else {
            self.settings.remove_model_installation(descriptor.id)?;
            None
        };
        let job = self.latest_job(descriptor.id)?;
        let status = if installed {
            ModelInstallationStatus::Installed
        } else if job.as_ref().is_some_and(|job| {
            matches!(
                job.status,
                ModelDownloadStatus::Queued
                    | ModelDownloadStatus::Downloading
                    | ModelDownloadStatus::Verifying
            )
        }) {
            ModelInstallationStatus::Downloading
        } else if job
            .as_ref()
            .is_some_and(|job| job.status == ModelDownloadStatus::Failed)
        {
            ModelInstallationStatus::Failed
        } else if !compatibility.disk_compatible {
            ModelInstallationStatus::Incompatible
        } else {
            ModelInstallationStatus::NotInstalled
        };
        let last_error = job.as_ref().and_then(|job| job.error.clone());
        Ok(ModelInstallation {
            descriptor: transport_descriptor(descriptor),
            status,
            installed_bytes: if installed { descriptor.disk_bytes } else { 0 },
            installed_at,
            selected_as_default: settings.default_transcription_model_id() == descriptor.id,
            available_backends: vec![ModelBackend::Cpu],
            compatibility: transport_compatibility(compatibility),
            download_job: job,
            last_error,
        })
    }

    fn recover_interrupted_jobs(&self) -> Result<(), AppError> {
        let interrupted = self
            .jobs
            .lock()
            .map_err(lock_error)?
            .values()
            .filter(|job| job.is_interrupted_running())
            .cloned()
            .collect::<Vec<_>>();
        for job in interrupted {
            let bytes = self
                .manager
                .lock()
                .map_err(lock_error)?
                .resumable_bytes(&job.model_id)
                .unwrap_or(0);
            self.transition_job(job.request_id, ModelDownloadStatus::Paused, bytes, None)?;
        }
        Ok(())
    }

    fn latest_job(&self, model_id: &str) -> Result<Option<ModelDownloadJob>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(lock_error)?
            .values()
            .filter(|job| job.model_id == model_id)
            .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
            .cloned())
    }

    fn transition_job(
        &self,
        request_id: RequestId,
        status: ModelDownloadStatus,
        bytes: u64,
        error: Option<AppError>,
    ) -> Result<ModelDownloadJob, AppError> {
        let mut jobs = self.jobs.lock().map_err(lock_error)?;
        let job = jobs
            .get_mut(&request_id)
            .ok_or_else(|| AppError::model_error("model_download_not_found"))?;
        job.status = status;
        job.bytes_downloaded = bytes.min(job.total_bytes);
        job.resumable = matches!(
            status,
            ModelDownloadStatus::Paused
                | ModelDownloadStatus::Cancelled
                | ModelDownloadStatus::Failed
        ) && job.bytes_downloaded > 0;
        job.updated_at = now_rfc3339()?;
        job.error = error;
        job.validate().map_err(AppError::model_operation_failed)?;
        let snapshot = job.clone();
        drop(jobs);
        self.settings.save_model_job(&snapshot)?;
        Ok(snapshot)
    }

    fn store_job(&self, job: ModelDownloadJob) -> Result<(), AppError> {
        self.settings.save_model_job(&job)?;
        self.jobs
            .lock()
            .map_err(lock_error)?
            .insert(job.request_id, job);
        Ok(())
    }
}

fn transport_descriptor(value: &ModelDescriptor) -> TransportDescriptor {
    TransportDescriptor {
        id: value.id.to_owned(),
        engine: value.engine.to_owned(),
        name: value.name.to_owned(),
        source_url: value.source_url.to_owned(),
        source_revision: value.source_revision.to_owned(),
        file_name: value.file_name.to_owned(),
        sha256: value.sha256.to_owned(),
        download_bytes: value.download_bytes,
        disk_bytes: value.disk_bytes,
        languages: value
            .languages
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        approximate_memory_bytes: value.approximate_memory_bytes,
        performance_class: match value.performance_class {
            "fast" => PerformanceClass::Fast,
            "balanced" => PerformanceClass::Balanced,
            _ => PerformanceClass::Accurate,
        },
        backends: value
            .backends
            .iter()
            .filter_map(|value| match *value {
                "cpu" => Some(ModelBackend::Cpu),
                "vulkan" => Some(ModelBackend::Vulkan),
                _ => None,
            })
            .collect(),
        license_spdx: value.license_spdx.to_owned(),
        license_url: value.license_url.to_owned(),
    }
}

fn transport_compatibility(value: Compatibility) -> ModelCompatibility {
    ModelCompatibility {
        available_disk_bytes: value.available_disk_bytes,
        required_disk_bytes: value.required_disk_bytes,
        available_memory_bytes: value.available_memory_bytes,
        approximate_memory_bytes: value.approximate_memory_bytes,
        disk_compatible: value.disk_compatible,
        memory_compatible: value.memory_compatible,
    }
}

fn map_error(error: ModelManagerError) -> AppError {
    let code = match error {
        ModelManagerError::UnknownModel => "model_unknown",
        ModelManagerError::InsufficientDisk => "insufficient_model_disk",
        ModelManagerError::ModelNotInstalled => "model_not_installed",
        ModelManagerError::InstalledModelInvalid => "installed_model_invalid",
        ModelManagerError::SelectedModelCannotBeDeleted => "selected_model_cannot_be_deleted",
        ModelManagerError::DownloadInterrupted => "model_download_interrupted",
        _ => "model_operation_failed",
    };
    AppError::model_error(code)
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> AppError {
    AppError::model_operation_failed("The local model service lock was poisoned.")
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::atomic::AtomicBool};

    use crate::{domain::ModelDownloadStatus, persistence::SettingsService};

    use super::{
        Compatibility, InstallOutcome, InstallProgress, ModelManagerError, ModelService,
        ModelStorageBackend,
    };

    struct FakeBackend {
        installed: HashSet<String>,
        resumable_bytes: u64,
        outcome: InstallOutcome,
        verified_path: std::path::PathBuf,
    }

    impl FakeBackend {
        fn new(outcome: InstallOutcome) -> Self {
            Self {
                installed: HashSet::new(),
                resumable_bytes: 0,
                outcome,
                verified_path: std::path::PathBuf::from("C:\\verified-models\\model.bin"),
            }
        }
    }

    impl ModelStorageBackend for FakeBackend {
        fn compatibility(&self, model_id: &str) -> Result<Compatibility, ModelManagerError> {
            let total = super::catalog::find(model_id)
                .ok_or(ModelManagerError::UnknownModel)?
                .download_bytes;
            Ok(Compatibility {
                available_disk_bytes: total * 4,
                required_disk_bytes: total,
                available_memory_bytes: 8 * 1024 * 1024 * 1024,
                approximate_memory_bytes: 512 * 1024 * 1024,
                disk_compatible: true,
                memory_compatible: true,
            })
        }

        fn is_installed(&self, model_id: &str) -> Result<bool, ModelManagerError> {
            Ok(self.installed.contains(model_id))
        }

        fn resumable_bytes(&self, _model_id: &str) -> Result<u64, ModelManagerError> {
            Ok(self.resumable_bytes)
        }

        fn install(
            &mut self,
            model_id: &str,
            _cancel: &AtomicBool,
            progress: &mut dyn FnMut(InstallProgress),
        ) -> Result<InstallOutcome, ModelManagerError> {
            let total = super::catalog::find(model_id).unwrap().download_bytes;
            progress(InstallProgress::Downloading {
                bytes_downloaded: total / 2,
                total_bytes: total,
            });
            match self.outcome {
                InstallOutcome::Installed | InstallOutcome::AlreadyInstalled => {
                    progress(InstallProgress::Verifying { total_bytes: total });
                    self.installed.insert(model_id.to_owned());
                    self.resumable_bytes = 0;
                }
                InstallOutcome::Cancelled => self.resumable_bytes = total / 2,
            }
            Ok(self.outcome)
        }

        fn select(&mut self, model_id: &str) -> Result<(), ModelManagerError> {
            self.installed
                .contains(model_id)
                .then_some(())
                .ok_or(ModelManagerError::ModelNotInstalled)
        }

        fn delete(&mut self, model_id: &str) -> Result<u64, ModelManagerError> {
            self.installed.remove(model_id);
            self.resumable_bytes = 0;
            Ok(1)
        }

        fn verified_model_path(
            &self,
            model_id: &str,
        ) -> Result<std::path::PathBuf, ModelManagerError> {
            self.installed
                .contains(model_id)
                .then(|| self.verified_path.clone())
                .ok_or(ModelManagerError::ModelNotInstalled)
        }
    }

    #[test]
    fn queue_is_single_flight_and_restart_recovers_running_job_as_paused() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let service = ModelService::open(app_data.path(), settings.clone()).unwrap();
        let queued = service
            .queue_download("whisper-tiny-multilingual", crate::domain::RequestId::new())
            .unwrap();
        assert_eq!(queued.status, ModelDownloadStatus::Queued);
        assert_eq!(
            service
                .queue_download("whisper-base-multilingual", crate::domain::RequestId::new())
                .unwrap_err()
                .code,
            "model_download_in_progress"
        );
        drop(service);

        let recovered = ModelService::open(app_data.path(), settings).unwrap();
        let tiny = recovered
            .list()
            .unwrap()
            .into_iter()
            .find(|model| model.descriptor.id == "whisper-tiny-multilingual")
            .unwrap();
        assert_eq!(
            tiny.download_job.unwrap().status,
            ModelDownloadStatus::Paused
        );
    }

    #[test]
    fn fake_backend_drives_resumed_progress_to_verified_installation() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let mut backend = FakeBackend::new(InstallOutcome::Installed);
        backend.resumable_bytes = 1024;
        let service = ModelService::from_backend(settings, Box::new(backend)).unwrap();
        let job = service
            .queue_resume("whisper-tiny-multilingual", crate::domain::RequestId::new())
            .unwrap();
        let mut statuses = Vec::new();
        service
            .run_download(job.request_id, |event| {
                statuses.push(event.payload.job.status)
            })
            .unwrap();
        assert!(statuses.contains(&ModelDownloadStatus::Downloading));
        assert!(statuses.contains(&ModelDownloadStatus::Verifying));
        assert_eq!(statuses.last(), Some(&ModelDownloadStatus::Completed));
        assert_eq!(
            service
                .list()
                .unwrap()
                .into_iter()
                .find(|model| model.descriptor.id == "whisper-tiny-multilingual")
                .unwrap()
                .status,
            crate::domain::ModelInstallationStatus::Installed
        );
    }

    #[test]
    fn fake_backend_cancellation_is_resumable_and_selection_conflict_does_not_poison_deletion() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let cancelled = ModelService::from_backend(
            settings.clone(),
            Box::new(FakeBackend::new(InstallOutcome::Cancelled)),
        )
        .unwrap();
        let job = cancelled
            .queue_download("whisper-tiny-multilingual", crate::domain::RequestId::new())
            .unwrap();
        cancelled.run_download(job.request_id, |_| {}).unwrap();
        let terminal = cancelled
            .latest_job("whisper-tiny-multilingual")
            .unwrap()
            .unwrap();
        assert_eq!(terminal.status, ModelDownloadStatus::Cancelled);
        assert!(terminal.resumable);

        let mut installed_backend = FakeBackend::new(InstallOutcome::Installed);
        installed_backend
            .installed
            .insert("whisper-tiny-multilingual".to_owned());
        let installed = ModelService::from_backend(settings, Box::new(installed_backend)).unwrap();
        assert_eq!(
            installed
                .set_default("whisper-tiny-multilingual", 99)
                .unwrap_err()
                .code,
            "settings_revision_conflict"
        );
        let deleted = installed.delete("whisper-tiny-multilingual").unwrap();
        assert_eq!(
            deleted.status,
            crate::domain::ModelInstallationStatus::NotInstalled
        );
        assert!(deleted.download_job.is_none());
    }

    #[test]
    fn transcription_resolution_reverifies_the_configured_default_inside_rust() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let mut backend = FakeBackend::new(InstallOutcome::Installed);
        backend
            .installed
            .insert("whisper-base-multilingual".to_owned());
        let service = ModelService::from_backend(settings, Box::new(backend)).unwrap();

        let artifact = service.resolve_default_for_transcription().unwrap();
        assert_eq!(artifact.model_id, "whisper-base-multilingual");
        assert_eq!(
            artifact.path,
            std::path::PathBuf::from("C:\\verified-models\\model.bin")
        );

        let uninstalled_app_data = tempfile::tempdir().unwrap();
        let uninstalled_documents = tempfile::tempdir().unwrap();
        let uninstalled = ModelService::from_backend(
            SettingsService::open(
                uninstalled_app_data.path().to_path_buf(),
                uninstalled_documents.path().to_path_buf(),
            )
            .unwrap(),
            Box::new(FakeBackend::new(InstallOutcome::Installed)),
        )
        .unwrap();
        assert_eq!(
            uninstalled
                .resolve_default_for_transcription()
                .unwrap_err()
                .code,
            "model_not_installed"
        );
    }
}
