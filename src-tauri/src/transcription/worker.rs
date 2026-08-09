use std::{
    ffi::{OsStr, OsString, c_void},
    io::{self, Read, Write},
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::AsRawHandle,
        process::CommandExt,
    },
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
    System::Threading::{GetCurrentProcess, TerminateProcess},
};

use crate::audio::AudioSource;

use super::{
    MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS, TranscriptSegment, TranscriptionEngine,
    TranscriptionError, TranscriptionRequest, TranscriptionResult, WhisperBackend,
    WhisperModelKind,
    model::MAX_TRANSCRIPTION_SAMPLES,
    whisper::{WhisperConfig, WhisperEngine},
};

pub(crate) const WORKER_MODE_ARGUMENT: &str = "--kokorokoe-vulkan-worker";
const PROTOCOL_VERSION: u16 = 1;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_PATH_UNITS: usize = 32_767;
const MAX_LANGUAGE_BYTES: usize = 15;
const IO_POLL_INTERVAL: Duration = Duration::from_millis(10);
const WORKER_EXIT_PROTOCOL: i32 = 20;
const WORKER_EXIT_STARTUP: i32 = 21;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub(crate) struct VulkanWorkerConfig {
    pub(crate) executable_path: PathBuf,
    pub(crate) adapter_path: PathBuf,
    pub(crate) model_path: PathBuf,
    pub(crate) model_kind: WhisperModelKind,
    pub(crate) threads: usize,
    pub(crate) environment: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorkerTimeouts {
    pub(crate) startup: Duration,
    pub(crate) write: Duration,
    pub(crate) inference: Duration,
}

impl Default for WorkerTimeouts {
    fn default() -> Self {
        Self {
            startup: Duration::from_secs(30),
            write: Duration::from_secs(5),
            inference: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerFailureKind {
    Startup,
    Protocol,
    WriteTimeout,
    InferenceTimeout,
    Cancelled,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerFailure {
    pub(crate) kind: WorkerFailureKind,
}

impl WorkerFailure {
    pub(crate) const fn new(kind: WorkerFailureKind) -> Self {
        Self { kind }
    }

    pub(crate) const fn code(self) -> &'static str {
        match self.kind {
            WorkerFailureKind::Startup => "transcription_worker_startup_failed",
            WorkerFailureKind::Protocol => "transcription_worker_protocol_failed",
            WorkerFailureKind::WriteTimeout => "transcription_worker_write_timeout",
            WorkerFailureKind::InferenceTimeout => "transcription_worker_inference_timeout",
            WorkerFailureKind::Cancelled => "transcription_worker_cancelled",
            WorkerFailureKind::Terminated => "transcription_worker_terminated",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WorkerFallbackDiagnostics {
    pub(crate) startup_failures: u64,
    pub(crate) protocol_failures: u64,
    pub(crate) write_timeouts: u64,
    pub(crate) inference_timeouts: u64,
    pub(crate) cancellations: u64,
    pub(crate) terminations: u64,
    pub(crate) accelerated_results: u64,
    pub(crate) cpu_load_attempts: u64,
    pub(crate) cpu_fallback_attempts: u64,
    pub(crate) cpu_fallback_results: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WireModelKind {
    Tiny,
    Base,
}

impl From<WhisperModelKind> for WireModelKind {
    fn from(value: WhisperModelKind) -> Self {
        match value {
            WhisperModelKind::Tiny => Self::Tiny,
            WhisperModelKind::Base => Self::Base,
        }
    }
}

impl From<WireModelKind> for WhisperModelKind {
    fn from(value: WireModelKind) -> Self {
        match value {
            WireModelKind::Tiny => Self::Tiny,
            WireModelKind::Base => Self::Base,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WireFailureCode {
    AdapterUnavailable,
    AdapterIncompatible,
    ModelUnavailable,
    ModelLoadFailed,
    BackendUnavailable,
    InvalidAudio,
    InvalidTimeline,
    InferenceFailed,
    InvalidNativeResult,
}

impl From<TranscriptionError> for WireFailureCode {
    fn from(value: TranscriptionError) -> Self {
        match value {
            TranscriptionError::AdapterUnavailable => Self::AdapterUnavailable,
            TranscriptionError::AdapterIncompatible => Self::AdapterIncompatible,
            TranscriptionError::ModelUnavailable => Self::ModelUnavailable,
            TranscriptionError::ModelLoadFailed => Self::ModelLoadFailed,
            TranscriptionError::BackendUnavailable => Self::BackendUnavailable,
            TranscriptionError::InvalidAudio => Self::InvalidAudio,
            TranscriptionError::InvalidTimeline => Self::InvalidTimeline,
            TranscriptionError::InferenceFailed => Self::InferenceFailed,
            TranscriptionError::InvalidNativeResult => Self::InvalidNativeResult,
            TranscriptionError::WorkerStartupFailed
            | TranscriptionError::WorkerProtocolFailed
            | TranscriptionError::WorkerWriteTimeout
            | TranscriptionError::WorkerInferenceTimeout
            | TranscriptionError::WorkerCancelled
            | TranscriptionError::WorkerTerminated => Self::InferenceFailed,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ClientMessage {
    Startup {
        protocol_version: u16,
        adapter_path_utf16: Vec<u16>,
        model_path_utf16: Vec<u16>,
        model_kind: WireModelKind,
        threads: usize,
    },
    Infer {
        protocol_version: u16,
        request_id: u64,
        source: AudioSource,
        start_ms: u64,
        end_ms: u64,
        samples: Vec<f32>,
    },
    Shutdown {
        protocol_version: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireSegment {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ServerMessage {
    Hello {
        protocol_version: u16,
        backend: String,
        max_in_flight: u8,
        max_samples: usize,
    },
    Result {
        protocol_version: u16,
        request_id: u64,
        source: AudioSource,
        start_ms: u64,
        end_ms: u64,
        language: String,
        text: String,
        segments: Vec<WireSegment>,
    },
    Failure {
        protocol_version: u16,
        request_id: Option<u64>,
        code: WireFailureCode,
    },
}

#[derive(Debug)]
enum ProtocolError {
    Io,
    Oversized,
    Invalid,
}

fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), ProtocolError> {
    let payload = serde_json::to_vec(value).map_err(|_| ProtocolError::Invalid)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Oversized);
    }
    let length = u32::try_from(payload.len()).map_err(|_| ProtocolError::Oversized)?;
    writer
        .write_all(&length.to_le_bytes())
        .and_then(|()| writer.write_all(&payload))
        .and_then(|()| writer.flush())
        .map_err(|_| ProtocolError::Io)
}

fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<T, ProtocolError> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|_| ProtocolError::Io)?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(ProtocolError::Oversized);
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|_| ProtocolError::Io)?;
    serde_json::from_slice(&payload).map_err(|_| ProtocolError::Invalid)
}

fn path_to_wire(path: &OsStr) -> Result<Vec<u16>, WorkerFailure> {
    let units: Vec<u16> = path.encode_wide().collect();
    if units.is_empty() || units.len() > MAX_PATH_UNITS || units.contains(&0) {
        return Err(WorkerFailure::new(WorkerFailureKind::Startup));
    }
    Ok(units)
}

fn wire_to_path(units: Vec<u16>) -> Result<PathBuf, ProtocolError> {
    if units.is_empty() || units.len() > MAX_PATH_UNITS || units.contains(&0) {
        return Err(ProtocolError::Invalid);
    }
    Ok(PathBuf::from(OsString::from_wide(&units)))
}

fn validate_wire_result(
    request_id: u64,
    request: TranscriptionRequest<'_>,
    message: ServerMessage,
) -> Result<TranscriptionResult, WorkerFailure> {
    let ServerMessage::Result {
        protocol_version,
        request_id: returned_id,
        source,
        start_ms,
        end_ms,
        language,
        text,
        segments,
    } = message
    else {
        return Err(WorkerFailure::new(WorkerFailureKind::Protocol));
    };
    if protocol_version != PROTOCOL_VERSION
        || returned_id != request_id
        || source != request.source
        || start_ms != request.start_ms
        || end_ms != request.end_ms
        || language.is_empty()
        || language.len() > MAX_LANGUAGE_BYTES
        || !language
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        || text.len() > MAX_TRANSCRIPT_BYTES
        || segments.len() > MAX_TRANSCRIPT_SEGMENTS
    {
        return Err(WorkerFailure::new(WorkerFailureKind::Protocol));
    }

    let mut previous_end = start_ms;
    let mut segment_bytes = 0_usize;
    let mut concatenated = String::new();
    let mut validated = Vec::with_capacity(segments.len());
    for segment in segments {
        segment_bytes = segment_bytes.saturating_add(segment.text.len());
        if segment_bytes > MAX_TRANSCRIPT_BYTES
            || segment.start_ms < previous_end
            || segment.end_ms < segment.start_ms
            || segment.end_ms > end_ms
        {
            return Err(WorkerFailure::new(WorkerFailureKind::Protocol));
        }
        concatenated.push_str(&segment.text);
        previous_end = segment.end_ms;
        validated.push(TranscriptSegment {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            text: segment.text,
        });
    }
    if concatenated.trim() != text {
        return Err(WorkerFailure::new(WorkerFailureKind::Protocol));
    }
    Ok(TranscriptionResult {
        source,
        utterance_start_ms: start_ms,
        utterance_end_ms: end_ms,
        language,
        text,
        segments: validated,
    })
}

struct JobGuard {
    handle: HANDLE,
}

impl JobGuard {
    fn assign(child: &Child) -> Result<Self, WorkerFailure> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(WorkerFailure::new(WorkerFailureKind::Startup));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } != 0;
        let assigned = configured
            && unsafe { AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE) } != 0;
        if !assigned {
            unsafe {
                CloseHandle(handle);
            }
            return Err(WorkerFailure::new(WorkerFailureKind::Startup));
        }
        Ok(Self { handle })
    }

    fn terminate(&self) {
        if !self.handle.is_null() {
            unsafe {
                TerminateJobObject(self.handle, 1);
            }
        }
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
            self.handle = ptr::null_mut();
        }
    }
}

pub(crate) trait AcceleratedWorker {
    fn transcribe_accelerated(
        &mut self,
        request: TranscriptionRequest<'_>,
        cancelled: &AtomicBool,
    ) -> Result<TranscriptionResult, WorkerFailure>;

    fn shutdown(&mut self);
}

pub(crate) struct VulkanWorkerProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    job: JobGuard,
    timeouts: WorkerTimeouts,
    next_request_id: u64,
    terminated: bool,
}

impl VulkanWorkerProcess {
    pub(crate) fn spawn(
        config: VulkanWorkerConfig,
        timeouts: WorkerTimeouts,
    ) -> Result<Self, WorkerFailure> {
        if !config.executable_path.is_file() || !(1..=64).contains(&config.threads) {
            return Err(WorkerFailure::new(WorkerFailureKind::Startup));
        }
        let startup = ClientMessage::Startup {
            protocol_version: PROTOCOL_VERSION,
            adapter_path_utf16: path_to_wire(config.adapter_path.as_os_str())?,
            model_path_utf16: path_to_wire(config.model_path.as_os_str())?,
            model_kind: config.model_kind.into(),
            threads: config.threads,
        };
        let mut command = Command::new(config.executable_path);
        command
            .arg(WORKER_MODE_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        for (name, value) in config.environment {
            command.env(name, value);
        }
        let mut child = command
            .spawn()
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::Startup))?;
        let job = match JobGuard::assign(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::Startup))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::Startup))?;
        let mut worker = Self {
            child,
            stdin: Some(stdin),
            stdout: Some(stdout),
            job,
            timeouts,
            next_request_id: 1,
            terminated: false,
        };
        let never_cancel = AtomicBool::new(false);
        if let Err(error) = worker.write_client(startup, timeouts.write, &never_cancel) {
            worker.terminate();
            return Err(WorkerFailure::new(match error.kind {
                WorkerFailureKind::Cancelled => WorkerFailureKind::Startup,
                other => other,
            }));
        }
        let hello = match worker.read_server(timeouts.startup, &never_cancel) {
            Ok(message) => message,
            Err(error) => {
                worker.terminate();
                let kind = match error.kind {
                    WorkerFailureKind::Protocol => WorkerFailureKind::Protocol,
                    _ => WorkerFailureKind::Startup,
                };
                return Err(WorkerFailure::new(kind));
            }
        };
        match hello {
            ServerMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                backend,
                max_in_flight: 1,
                max_samples: MAX_TRANSCRIPTION_SAMPLES,
            } if backend == WhisperBackend::Vulkan.as_str() => Ok(worker),
            ServerMessage::Failure { .. } => {
                worker.terminate();
                Err(WorkerFailure::new(WorkerFailureKind::Startup))
            }
            _ => {
                worker.terminate();
                Err(WorkerFailure::new(WorkerFailureKind::Protocol))
            }
        }
    }

    fn write_client(
        &mut self,
        message: ClientMessage,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<(), WorkerFailure> {
        let mut stdin = self
            .stdin
            .take()
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::Terminated))?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let writer = thread::spawn(move || {
            let result = write_frame(&mut stdin, &message);
            let _ = sender.send((stdin, result));
        });
        let outcome = wait_for_io(receiver, timeout, cancelled);
        match outcome {
            IoWait::Complete((stdin, Ok(()))) => {
                let _ = writer.join();
                self.stdin = Some(stdin);
                Ok(())
            }
            IoWait::Complete((_stdin, Err(_))) => {
                let _ = writer.join();
                Err(WorkerFailure::new(WorkerFailureKind::Terminated))
            }
            IoWait::Disconnected => {
                let _ = writer.join();
                Err(WorkerFailure::new(WorkerFailureKind::Terminated))
            }
            IoWait::Cancelled => {
                self.terminate();
                let _ = writer.join();
                Err(WorkerFailure::new(WorkerFailureKind::Cancelled))
            }
            IoWait::TimedOut => {
                self.terminate();
                let _ = writer.join();
                Err(WorkerFailure::new(WorkerFailureKind::WriteTimeout))
            }
        }
    }

    fn read_server(
        &mut self,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<ServerMessage, WorkerFailure> {
        let mut stdout = self
            .stdout
            .take()
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::Terminated))?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let result = read_frame(&mut stdout);
            let _ = sender.send((stdout, result));
        });
        let deadline = Instant::now() + timeout;
        let outcome = loop {
            if cancelled.load(Ordering::Acquire) {
                break IoWait::Cancelled;
            }
            if self.child.try_wait().ok().flatten().is_some() {
                break IoWait::Disconnected;
            }
            let now = Instant::now();
            if now >= deadline {
                break IoWait::TimedOut;
            }
            let wait = IO_POLL_INTERVAL.min(deadline.saturating_duration_since(now));
            match receiver.recv_timeout(wait) {
                Ok(value) => break IoWait::Complete(value),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break IoWait::Disconnected,
            }
        };
        match outcome {
            IoWait::Complete((stdout, Ok(message))) => {
                let _ = reader.join();
                self.stdout = Some(stdout);
                Ok(message)
            }
            IoWait::Complete((_stdout, Err(ProtocolError::Invalid | ProtocolError::Oversized))) => {
                let _ = reader.join();
                Err(WorkerFailure::new(WorkerFailureKind::Protocol))
            }
            IoWait::Complete((_stdout, Err(ProtocolError::Io))) => {
                let _ = reader.join();
                Err(WorkerFailure::new(WorkerFailureKind::Terminated))
            }
            IoWait::Disconnected => {
                let _ = reader.join();
                Err(WorkerFailure::new(WorkerFailureKind::Terminated))
            }
            IoWait::Cancelled => {
                self.terminate();
                let _ = reader.join();
                Err(WorkerFailure::new(WorkerFailureKind::Cancelled))
            }
            IoWait::TimedOut => {
                self.terminate();
                let _ = reader.join();
                Err(WorkerFailure::new(WorkerFailureKind::InferenceTimeout))
            }
        }
    }

    fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        self.stdin.take();
        self.stdout.take();
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl AcceleratedWorker for VulkanWorkerProcess {
    fn transcribe_accelerated(
        &mut self,
        request: TranscriptionRequest<'_>,
        cancelled: &AtomicBool,
    ) -> Result<TranscriptionResult, WorkerFailure> {
        let request = request
            .validate()
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::Protocol))?;
        if cancelled.load(Ordering::Acquire) {
            return Err(WorkerFailure::new(WorkerFailureKind::Cancelled));
        }
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        let message = ClientMessage::Infer {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            source: request.source,
            start_ms: request.start_ms,
            end_ms: request.end_ms,
            samples: request.samples.to_vec(),
        };
        self.write_client(message, self.timeouts.write, cancelled)?;
        let response = self.read_server(self.timeouts.inference, cancelled)?;
        match response {
            ServerMessage::Failure {
                protocol_version: PROTOCOL_VERSION,
                request_id: Some(returned_id),
                ..
            } if returned_id == request_id => {
                Err(WorkerFailure::new(WorkerFailureKind::Terminated))
            }
            message => validate_wire_result(request_id, request, message),
        }
    }

    fn shutdown(&mut self) {
        if self.terminated {
            return;
        }
        let never_cancel = AtomicBool::new(false);
        let _ = self.write_client(
            ClientMessage::Shutdown {
                protocol_version: PROTOCOL_VERSION,
            },
            self.timeouts.write,
            &never_cancel,
        );
        self.terminate();
    }
}

impl Drop for VulkanWorkerProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

enum IoWait<T> {
    Complete(T),
    Cancelled,
    TimedOut,
    Disconnected,
}

fn wait_for_io<T>(
    receiver: mpsc::Receiver<T>,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> IoWait<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return IoWait::Cancelled;
        }
        let now = Instant::now();
        if now >= deadline {
            return IoWait::TimedOut;
        }
        let wait = IO_POLL_INTERVAL.min(deadline.saturating_duration_since(now));
        match receiver.recv_timeout(wait) {
            Ok(value) => return IoWait::Complete(value),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return IoWait::Disconnected,
        }
    }
}

pub(crate) struct SupervisedFallbackEngine<A, C, F> {
    accelerated: Option<A>,
    cpu: Option<C>,
    cpu_factory: Option<F>,
    diagnostics: WorkerFallbackDiagnostics,
}

impl<A, C, F> SupervisedFallbackEngine<A, C, F> {
    pub(crate) fn new(accelerated: Result<A, WorkerFailure>, cpu_factory: F) -> Self {
        let (accelerated, diagnostics) = match accelerated {
            Ok(worker) => (Some(worker), WorkerFallbackDiagnostics::default()),
            Err(error) => {
                let mut diagnostics = WorkerFallbackDiagnostics::default();
                record_worker_failure(&mut diagnostics, error.kind);
                (None, diagnostics)
            }
        };
        Self {
            accelerated,
            cpu: None,
            cpu_factory: Some(cpu_factory),
            diagnostics,
        }
    }

    pub(crate) const fn diagnostics(&self) -> WorkerFallbackDiagnostics {
        self.diagnostics
    }
}

impl<A, C, F> SupervisedFallbackEngine<A, C, F>
where
    A: AcceleratedWorker,
    C: TranscriptionEngine,
    F: FnOnce() -> Result<C, TranscriptionError>,
{
    pub(crate) fn transcribe_with_cancel(
        &mut self,
        request: TranscriptionRequest<'_>,
        cancelled: &AtomicBool,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let request = request.validate()?;
        if cancelled.load(Ordering::Acquire) {
            return Err(TranscriptionError::WorkerCancelled);
        }
        if let Some(worker) = self.accelerated.as_mut() {
            match worker.transcribe_accelerated(request, cancelled) {
                Ok(result) => {
                    self.diagnostics.accelerated_results =
                        self.diagnostics.accelerated_results.saturating_add(1);
                    return Ok(result);
                }
                Err(error) => {
                    record_worker_failure(&mut self.diagnostics, error.kind);
                    if let Some(mut worker) = self.accelerated.take() {
                        worker.shutdown();
                    }
                    if error.kind == WorkerFailureKind::Cancelled {
                        return Err(TranscriptionError::WorkerCancelled);
                    }
                }
            }
        }

        if self.cpu.is_none() {
            self.diagnostics.cpu_load_attempts =
                self.diagnostics.cpu_load_attempts.saturating_add(1);
            let factory = self
                .cpu_factory
                .take()
                .ok_or(TranscriptionError::ModelLoadFailed)?;
            self.cpu = Some(factory()?);
        }
        self.diagnostics.cpu_fallback_attempts =
            self.diagnostics.cpu_fallback_attempts.saturating_add(1);
        let result = self
            .cpu
            .as_mut()
            .expect("CPU engine must exist after successful lazy load")
            .transcribe(request)?;
        self.diagnostics.cpu_fallback_results =
            self.diagnostics.cpu_fallback_results.saturating_add(1);
        Ok(result)
    }
}

impl<A, C, F> TranscriptionEngine for SupervisedFallbackEngine<A, C, F>
where
    A: AcceleratedWorker,
    C: TranscriptionEngine,
    F: FnOnce() -> Result<C, TranscriptionError>,
{
    fn transcribe(
        &mut self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        self.transcribe_with_cancel(request, &AtomicBool::new(false))
    }

    fn transcribe_with_cancel(
        &mut self,
        request: TranscriptionRequest<'_>,
        cancelled: &AtomicBool,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        SupervisedFallbackEngine::transcribe_with_cancel(self, request, cancelled)
    }
}

fn record_worker_failure(diagnostics: &mut WorkerFallbackDiagnostics, kind: WorkerFailureKind) {
    let counter = match kind {
        WorkerFailureKind::Startup => &mut diagnostics.startup_failures,
        WorkerFailureKind::Protocol => &mut diagnostics.protocol_failures,
        WorkerFailureKind::WriteTimeout => &mut diagnostics.write_timeouts,
        WorkerFailureKind::InferenceTimeout => &mut diagnostics.inference_timeouts,
        WorkerFailureKind::Cancelled => &mut diagnostics.cancellations,
        WorkerFailureKind::Terminated => &mut diagnostics.terminations,
    };
    *counter = counter.saturating_add(1);
}

pub(crate) fn run_worker_if_requested() {
    let exit_code = serve_worker();
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn serve_worker() -> i32 {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    #[cfg(debug_assertions)]
    let test_mode = std::env::var("KOKOROKOE_P3_008_TEST_MODE").ok();
    #[cfg(not(debug_assertions))]
    let test_mode: Option<String> = None;

    if test_mode.as_deref() == Some("hang_startup") {
        hang_forever();
    }
    if test_mode.as_deref() == Some("corrupt_hello") {
        let _ = writer.write_all(&1_u32.to_le_bytes());
        let _ = writer.write_all(b"{");
        let _ = writer.flush();
        return WORKER_EXIT_PROTOCOL;
    }

    let startup: ClientMessage = match read_frame(&mut reader) {
        Ok(message) => message,
        Err(_) => return WORKER_EXIT_PROTOCOL,
    };
    let ClientMessage::Startup {
        protocol_version: PROTOCOL_VERSION,
        adapter_path_utf16,
        model_path_utf16,
        model_kind,
        threads,
    } = startup
    else {
        return WORKER_EXIT_PROTOCOL;
    };
    let adapter_path = match wire_to_path(adapter_path_utf16) {
        Ok(path) => path,
        Err(_) => return WORKER_EXIT_PROTOCOL,
    };
    let model_path = match wire_to_path(model_path_utf16) {
        Ok(path) => path,
        Err(_) => return WORKER_EXIT_PROTOCOL,
    };
    let mut engine = match WhisperEngine::load(WhisperConfig::vulkan(
        adapter_path,
        model_path,
        model_kind.into(),
        threads,
    )) {
        Ok(engine) if engine.backend() == WhisperBackend::Vulkan => engine,
        Ok(_) => return WORKER_EXIT_STARTUP,
        Err(error) => {
            let _ = write_frame(
                &mut writer,
                &ServerMessage::Failure {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: None,
                    code: error.into(),
                },
            );
            return WORKER_EXIT_STARTUP;
        }
    };
    if write_frame(
        &mut writer,
        &ServerMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            backend: WhisperBackend::Vulkan.as_str().to_owned(),
            max_in_flight: 1,
            max_samples: MAX_TRANSCRIPTION_SAMPLES,
        },
    )
    .is_err()
    {
        return WORKER_EXIT_PROTOCOL;
    }
    if test_mode.as_deref() == Some("skip_request_read") {
        hang_forever();
    }

    loop {
        let message: ClientMessage = match read_frame(&mut reader) {
            Ok(message) => message,
            Err(_) => return WORKER_EXIT_PROTOCOL,
        };
        match message {
            ClientMessage::Shutdown {
                protocol_version: PROTOCOL_VERSION,
            } => return 0,
            ClientMessage::Infer {
                protocol_version: PROTOCOL_VERSION,
                request_id,
                source,
                start_ms,
                end_ms,
                samples,
            } => {
                if test_mode.as_deref() == Some("corrupt_response") {
                    let _ = writer.write_all(&1_u32.to_le_bytes());
                    let _ = writer.write_all(b"{");
                    let _ = writer.flush();
                    return WORKER_EXIT_PROTOCOL;
                }
                if test_mode.as_deref() == Some("terminate_inference") {
                    unsafe {
                        // SAFETY: this debug-only probe deliberately ends only the isolated worker
                        // process to exercise the parent's nonzero-exit recovery path.
                        TerminateProcess(GetCurrentProcess(), 86);
                    }
                    return WORKER_EXIT_PROTOCOL;
                }
                if test_mode.as_deref() == Some("hang_inference") {
                    hang_forever();
                }
                if test_mode.as_deref() == Some("spawn_descendant_hang") {
                    spawn_test_descendant();
                    hang_forever();
                }
                let request = TranscriptionRequest {
                    source,
                    start_ms,
                    end_ms,
                    samples: &samples,
                };
                let response = match engine.transcribe(request) {
                    Ok(result) => ServerMessage::Result {
                        protocol_version: PROTOCOL_VERSION,
                        request_id,
                        source: result.source,
                        start_ms: result.utterance_start_ms,
                        end_ms: result.utterance_end_ms,
                        language: result.language,
                        text: result.text,
                        segments: result
                            .segments
                            .into_iter()
                            .map(|segment| WireSegment {
                                start_ms: segment.start_ms,
                                end_ms: segment.end_ms,
                                text: segment.text,
                            })
                            .collect(),
                    },
                    Err(error) => ServerMessage::Failure {
                        protocol_version: PROTOCOL_VERSION,
                        request_id: Some(request_id),
                        code: error.into(),
                    },
                };
                if write_frame(&mut writer, &response).is_err() {
                    return WORKER_EXIT_PROTOCOL;
                }
            }
            _ => return WORKER_EXIT_PROTOCOL,
        }
    }
}

fn hang_forever() -> ! {
    loop {
        thread::park_timeout(Duration::from_secs(60));
    }
}

#[cfg(debug_assertions)]
fn spawn_test_descendant() {
    let Some(marker) = std::env::var_os("KOKOROKOE_P3_008_DESCENDANT_MARKER") else {
        return;
    };
    let Some(started) = std::env::var_os("KOKOROKOE_P3_008_DESCENDANT_STARTED") else {
        return;
    };
    let script = "Set-Content -LiteralPath $env:KOKOROKOE_P3_008_DESCENDANT_STARTED -Value started; Start-Sleep -Milliseconds 1500; Set-Content -LiteralPath $env:KOKOROKOE_P3_008_DESCENDANT_MARKER -Value survived";
    let _ = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("KOKOROKOE_P3_008_DESCENDANT_MARKER", marker)
        .env("KOKOROKOE_P3_008_DESCENDANT_STARTED", started)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

#[cfg(not(debug_assertions))]
fn spawn_test_descendant() {}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        sync::{Arc, Mutex},
    };

    use super::*;

    struct ScriptedWorker {
        outcome: Result<TranscriptionResult, WorkerFailure>,
        shutdowns: Arc<Mutex<u64>>,
    }

    impl AcceleratedWorker for ScriptedWorker {
        fn transcribe_accelerated(
            &mut self,
            _request: TranscriptionRequest<'_>,
            _cancelled: &AtomicBool,
        ) -> Result<TranscriptionResult, WorkerFailure> {
            self.outcome.clone()
        }

        fn shutdown(&mut self) {
            let mut shutdowns = self.shutdowns.lock().unwrap();
            *shutdowns += 1;
        }
    }

    #[derive(Default)]
    struct CountingCpu {
        calls: u64,
    }

    impl TranscriptionEngine for CountingCpu {
        fn transcribe(
            &mut self,
            request: TranscriptionRequest<'_>,
        ) -> Result<TranscriptionResult, TranscriptionError> {
            self.calls += 1;
            Ok(result_for(request, "cpu result"))
        }
    }

    fn request() -> TranscriptionRequest<'static> {
        TranscriptionRequest {
            source: AudioSource::SystemOutput,
            start_ms: 2_000,
            end_ms: 3_000,
            samples: &[0.25; 16_000],
        }
    }

    fn result_for(request: TranscriptionRequest<'_>, text: &str) -> TranscriptionResult {
        TranscriptionResult {
            source: request.source,
            utterance_start_ms: request.start_ms,
            utterance_end_ms: request.end_ms,
            language: "en".to_owned(),
            text: text.to_owned(),
            segments: vec![TranscriptSegment {
                start_ms: request.start_ms,
                end_ms: request.end_ms,
                text: text.to_owned(),
            }],
        }
    }

    #[test]
    fn protocol_round_trip_is_strict_bounded_and_source_labelled() {
        let message = ClientMessage::Infer {
            protocol_version: PROTOCOL_VERSION,
            request_id: 7,
            source: AudioSource::Microphone,
            start_ms: 10,
            end_ms: 20,
            samples: vec![0.0; 160],
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &message).unwrap();
        let parsed: ClientMessage = read_frame(&mut bytes.as_slice()).unwrap();
        let ClientMessage::Infer {
            request_id,
            source,
            samples,
            ..
        } = parsed
        else {
            panic!("expected inference frame");
        };
        assert_eq!(request_id, 7);
        assert_eq!(source, AudioSource::Microphone);
        assert_eq!(samples.len(), 160);

        let maximum = ClientMessage::Infer {
            protocol_version: PROTOCOL_VERSION,
            request_id: u64::MAX,
            source: AudioSource::SystemOutput,
            start_ms: 0,
            end_ms: 30_000,
            samples: vec![0.123_456; MAX_TRANSCRIPTION_SAMPLES],
        };
        let mut maximum_frame = Vec::new();
        write_frame(&mut maximum_frame, &maximum).unwrap();
        assert!(maximum_frame.len() <= MAX_FRAME_BYTES + 4);
        let parsed: ClientMessage = read_frame(&mut maximum_frame.as_slice()).unwrap();
        assert!(matches!(
            parsed,
            ClientMessage::Infer { samples, .. } if samples.len() == MAX_TRANSCRIPTION_SAMPLES
        ));

        let oversized = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
        assert!(matches!(
            read_frame::<ClientMessage>(&mut oversized.as_slice()),
            Err(ProtocolError::Oversized)
        ));
        let unknown = br#"{"type":"shutdown","protocol_version":1,"extra":true}"#;
        let mut framed = Vec::new();
        framed.extend_from_slice(&(unknown.len() as u32).to_le_bytes());
        framed.extend_from_slice(unknown);
        assert!(matches!(
            read_frame::<ClientMessage>(&mut framed.as_slice()),
            Err(ProtocolError::Invalid)
        ));
    }

    #[test]
    fn protocol_rejects_mismatched_or_overlapping_results() {
        let request = request();
        let mismatched = ServerMessage::Result {
            protocol_version: PROTOCOL_VERSION,
            request_id: 8,
            source: request.source,
            start_ms: request.start_ms,
            end_ms: request.end_ms,
            language: "en".to_owned(),
            text: "bad".to_owned(),
            segments: vec![WireSegment {
                start_ms: request.start_ms,
                end_ms: request.end_ms,
                text: "bad".to_owned(),
            }],
        };
        assert_eq!(
            validate_wire_result(7, request, mismatched)
                .unwrap_err()
                .kind,
            WorkerFailureKind::Protocol
        );

        let overlapping = ServerMessage::Result {
            protocol_version: PROTOCOL_VERSION,
            request_id: 7,
            source: request.source,
            start_ms: request.start_ms,
            end_ms: request.end_ms,
            language: "en".to_owned(),
            text: "ab".to_owned(),
            segments: vec![
                WireSegment {
                    start_ms: 2_000,
                    end_ms: 2_800,
                    text: "a".to_owned(),
                },
                WireSegment {
                    start_ms: 2_700,
                    end_ms: 3_000,
                    text: "b".to_owned(),
                },
            ],
        };
        assert_eq!(
            validate_wire_result(7, request, overlapping)
                .unwrap_err()
                .kind,
            WorkerFailureKind::Protocol
        );
    }

    #[test]
    fn failed_worker_is_terminated_before_cpu_load_and_returns_once() {
        let shutdowns = Arc::new(Mutex::new(0));
        let worker = ScriptedWorker {
            outcome: Err(WorkerFailure::new(WorkerFailureKind::Protocol)),
            shutdowns: Arc::clone(&shutdowns),
        };
        let shutdowns_at_load = Arc::clone(&shutdowns);
        let mut engine = SupervisedFallbackEngine::new(Ok(worker), move || {
            assert_eq!(*shutdowns_at_load.lock().unwrap(), 1);
            Ok(CountingCpu::default())
        });

        let result = engine.transcribe(request()).unwrap();
        assert_eq!(result.text, "cpu result");
        assert_eq!(engine.diagnostics().protocol_failures, 1);
        assert_eq!(engine.diagnostics().cpu_load_attempts, 1);
        assert_eq!(engine.diagnostics().cpu_fallback_attempts, 1);
        assert_eq!(engine.diagnostics().cpu_fallback_results, 1);
    }

    #[test]
    fn cancellation_terminates_worker_without_cpu_retry() {
        let shutdowns = Arc::new(Mutex::new(0));
        let worker = ScriptedWorker {
            outcome: Err(WorkerFailure::new(WorkerFailureKind::Cancelled)),
            shutdowns: Arc::clone(&shutdowns),
        };
        let mut engine = SupervisedFallbackEngine::new(Ok(worker), || Ok(CountingCpu::default()));

        assert_eq!(
            engine.transcribe(request()),
            Err(TranscriptionError::WorkerCancelled)
        );
        assert_eq!(*shutdowns.lock().unwrap(), 1);
        assert_eq!(engine.diagnostics().cancellations, 1);
        assert_eq!(engine.diagnostics().cpu_load_attempts, 0);
    }

    #[test]
    fn startup_failure_defers_cpu_load_until_the_first_request() {
        let loads = Arc::new(Mutex::new(0_u64));
        let loads_for_factory = Arc::clone(&loads);
        let mut engine = SupervisedFallbackEngine::<ScriptedWorker, _, _>::new(
            Err(WorkerFailure::new(WorkerFailureKind::Startup)),
            move || {
                *loads_for_factory.lock().unwrap() += 1;
                Ok(CountingCpu::default())
            },
        );
        assert_eq!(*loads.lock().unwrap(), 0);
        assert_eq!(engine.transcribe(request()).unwrap().text, "cpu result");
        assert_eq!(*loads.lock().unwrap(), 1);
        assert_eq!(engine.diagnostics().startup_failures, 1);
        assert_eq!(engine.diagnostics().cpu_fallback_results, 1);
    }

    #[test]
    fn worker_error_codes_are_fixed_and_path_free() {
        for kind in [
            WorkerFailureKind::Startup,
            WorkerFailureKind::Protocol,
            WorkerFailureKind::WriteTimeout,
            WorkerFailureKind::InferenceTimeout,
            WorkerFailureKind::Cancelled,
            WorkerFailureKind::Terminated,
        ] {
            let code = WorkerFailure::new(kind).code();
            assert!(code.starts_with("transcription_worker_"));
            assert!(!code.contains(['/', '\\']));
        }
    }

    fn p3_008_samples() -> Vec<f32> {
        let path = env::var_os("KOKOROKOE_P3_008_FIXTURE").expect("generated fixture path");
        let bytes = fs::read(path).expect("read generated f32 fixture");
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 4, 0);
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert!(samples.len() <= MAX_TRANSCRIPTION_SAMPLES);
        assert!(
            samples
                .iter()
                .all(|sample| sample.is_finite() && (-1.0..=1.0).contains(sample))
        );
        samples
    }

    fn p3_008_request(samples: &[f32]) -> TranscriptionRequest<'_> {
        let duration_ms = samples.len() as u64 * 1_000 / 16_000;
        TranscriptionRequest {
            source: AudioSource::SystemOutput,
            start_ms: 8_000,
            end_ms: 8_000 + duration_ms,
            samples,
        }
    }

    fn p3_008_config(
        mode: Option<&str>,
        extra_environment: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> VulkanWorkerConfig {
        let mut environment: Vec<_> = extra_environment.into_iter().collect();
        if let Some(mode) = mode {
            environment.push((
                OsString::from("KOKOROKOE_P3_008_TEST_MODE"),
                OsString::from(mode),
            ));
        }
        VulkanWorkerConfig {
            executable_path: PathBuf::from(
                env::var_os("KOKOROKOE_P3_008_WORKER_EXE").expect("worker executable path"),
            ),
            adapter_path: PathBuf::from(
                env::var_os("KOKOROKOE_WHISPER_ADAPTER").expect("adapter path"),
            ),
            model_path: PathBuf::from(
                env::var_os("KOKOROKOE_WHISPER_TINY_MODEL").expect("Tiny model path"),
            ),
            model_kind: WhisperModelKind::Tiny,
            threads: 8,
            environment,
        }
    }

    fn p3_008_cpu_factory() -> impl FnOnce() -> Result<WhisperEngine, TranscriptionError> {
        let adapter =
            PathBuf::from(env::var_os("KOKOROKOE_WHISPER_ADAPTER").expect("adapter path"));
        let model =
            PathBuf::from(env::var_os("KOKOROKOE_WHISPER_TINY_MODEL").expect("Tiny model path"));
        move || {
            WhisperEngine::load(WhisperConfig::cpu(
                adapter,
                model,
                WhisperModelKind::Tiny,
                8,
            ))
        }
    }

    fn p3_008_timeouts() -> WorkerTimeouts {
        WorkerTimeouts {
            startup: Duration::from_secs(5),
            write: Duration::from_millis(300),
            inference: Duration::from_millis(300),
        }
    }

    fn assert_cpu_recovery(
        samples: &[f32],
        config: VulkanWorkerConfig,
        expected_failure: WorkerFailureKind,
    ) -> WorkerFallbackDiagnostics {
        let worker = VulkanWorkerProcess::spawn(config, p3_008_timeouts());
        let mut engine = SupervisedFallbackEngine::new(worker, p3_008_cpu_factory());
        let result = engine
            .transcribe(p3_008_request(samples))
            .expect("failed worker attempt must recover once on CPU");
        assert!(!result.text.is_empty());
        let diagnostics = engine.diagnostics();
        assert_eq!(diagnostics.accelerated_results, 0);
        assert_eq!(diagnostics.cpu_load_attempts, 1);
        assert_eq!(diagnostics.cpu_fallback_attempts, 1);
        assert_eq!(diagnostics.cpu_fallback_results, 1);
        let observed = match expected_failure {
            WorkerFailureKind::Startup => diagnostics.startup_failures,
            WorkerFailureKind::Protocol => diagnostics.protocol_failures,
            WorkerFailureKind::WriteTimeout => diagnostics.write_timeouts,
            WorkerFailureKind::InferenceTimeout => diagnostics.inference_timeouts,
            WorkerFailureKind::Cancelled => diagnostics.cancellations,
            WorkerFailureKind::Terminated => diagnostics.terminations,
        };
        assert_eq!(
            observed, 1,
            "expected {expected_failure:?}; diagnostics={diagnostics:?}"
        );
        diagnostics
    }

    #[test]
    #[ignore = "explicit P3-008 native worker protocol and lifecycle gate"]
    fn vulkan_worker_protocol_lifecycle_and_cpu_recovery_gate() {
        let samples = p3_008_samples();
        let request = p3_008_request(&samples);

        let healthy =
            VulkanWorkerProcess::spawn(p3_008_config(None, []), WorkerTimeouts::default())
                .expect("worker must attest Vulkan");
        let mut healthy_engine = SupervisedFallbackEngine::new(
            Ok(healthy),
            || -> Result<CountingCpu, TranscriptionError> {
                panic!("healthy Vulkan must not load CPU")
            },
        );
        let accelerated = healthy_engine
            .transcribe(request)
            .expect("attested worker inference");
        assert!(!accelerated.text.is_empty());
        assert_eq!(healthy_engine.diagnostics().accelerated_results, 1);
        drop(healthy_engine);

        let missing_driver = env::temp_dir().join("kokorokoe-p3-008-missing-driver.json");
        assert!(!missing_driver.exists());
        let recoveries = [
            assert_cpu_recovery(
                &samples,
                p3_008_config(
                    None,
                    [(
                        OsString::from("VK_DRIVER_FILES"),
                        missing_driver.into_os_string(),
                    )],
                ),
                WorkerFailureKind::Startup,
            ),
            assert_cpu_recovery(
                &samples,
                p3_008_config(Some("corrupt_hello"), []),
                WorkerFailureKind::Protocol,
            ),
            assert_cpu_recovery(
                &samples,
                p3_008_config(Some("hang_startup"), []),
                WorkerFailureKind::Startup,
            ),
            assert_cpu_recovery(
                &samples,
                p3_008_config(Some("corrupt_response"), []),
                WorkerFailureKind::Protocol,
            ),
            assert_cpu_recovery(
                &samples,
                p3_008_config(Some("skip_request_read"), []),
                WorkerFailureKind::WriteTimeout,
            ),
            assert_cpu_recovery(
                &samples,
                p3_008_config(Some("hang_inference"), []),
                WorkerFailureKind::InferenceTimeout,
            ),
            assert_cpu_recovery(
                &samples,
                p3_008_config(Some("terminate_inference"), []),
                WorkerFailureKind::Terminated,
            ),
        ];
        assert_eq!(
            recoveries
                .iter()
                .map(|diagnostics| diagnostics.cpu_fallback_results)
                .sum::<u64>(),
            7
        );

        let marker_directory = tempfile::tempdir().unwrap();
        let started_marker = marker_directory.path().join("descendant-started.txt");
        let marker = marker_directory.path().join("descendant-survived.txt");
        let worker = VulkanWorkerProcess::spawn(
            p3_008_config(
                Some("spawn_descendant_hang"),
                [
                    (
                        OsString::from("KOKOROKOE_P3_008_DESCENDANT_MARKER"),
                        marker.clone().into_os_string(),
                    ),
                    (
                        OsString::from("KOKOROKOE_P3_008_DESCENDANT_STARTED"),
                        started_marker.clone().into_os_string(),
                    ),
                ],
            ),
            WorkerTimeouts {
                inference: Duration::from_secs(5),
                ..p3_008_timeouts()
            },
        )
        .expect("cancellation worker startup");
        let mut cancelled_engine =
            SupervisedFallbackEngine::new(Ok(worker), || Ok(CountingCpu::default()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancelled);
        let started_for_canceller = started_marker.clone();
        let canceller = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(4);
            while !started_for_canceller.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            let descendant_started = started_for_canceller.exists();
            signal.store(true, Ordering::Release);
            assert!(
                descendant_started,
                "test descendant must start before cancellation"
            );
        });
        assert_eq!(
            cancelled_engine.transcribe_with_cancel(request, &cancelled),
            Err(TranscriptionError::WorkerCancelled)
        );
        canceller.join().unwrap();
        assert_eq!(cancelled_engine.diagnostics().cancellations, 1);
        assert_eq!(cancelled_engine.diagnostics().cpu_load_attempts, 0);
        thread::sleep(Duration::from_secs(2));
        assert!(!marker.exists(), "job termination must kill descendants");

        println!(
            "protocol_version=1 vulkan_attested=true accelerated_results=1 failed_accelerated_attempts=7 cpu_results=7 cancelled_requests=1 duplicate_results=0 lost_results=0 protocol_corruption_rejected=true write_timeout_isolated=true inference_timeout_isolated=true crash_isolated=true descendant_cleanup=true"
        );
    }
}
