use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::TryRecvError;
use wasapi::{
    Device, DeviceEnumerator, Direction, Role, SampleType, StreamMode, WasapiError, WaveFormat,
    deinitialize, initialize_mta,
};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use super::{
    AudioDevice, AudioDeviceList, AudioDirection, AudioPacket, AudioPrototypeConfig,
    AudioPrototypeStartRequest, AudioPrototypeStatus, AudioSource, BoundedReceiver, BoundedSender,
    ChannelStatus, DeviceRole, DeviceSelection, EnqueueResult, NativeAudioFormat, NativeSampleType,
    PacketReceiver, PacketSender, ProcessedAudioChunk, ProcessingOutcome, PrototypeRunState,
    QpcEpoch, SourceProcessor, bounded_queue, packet_queue, qpc_ticks_to_100ns,
};

const EVENT_WAIT_MS: u32 = 100;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioPrototypeError {
    pub(crate) code: &'static str,
}

impl AudioPrototypeError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

#[derive(Clone, Default)]
pub(crate) struct AudioPrototypeService {
    running: Arc<Mutex<Option<RunningAudioPrototype>>>,
    last_status: Arc<Mutex<Option<AudioPrototypeStatus>>>,
}

impl AudioPrototypeService {
    pub(crate) fn list_devices(&self) -> Result<AudioDeviceList, AudioPrototypeError> {
        thread::Builder::new()
            .name("audio-device-enumeration".to_owned())
            .spawn(list_audio_devices)
            .map_err(|_| AudioPrototypeError::new("audio_enumeration_thread_unavailable"))?
            .join()
            .map_err(|_| AudioPrototypeError::new("audio_enumeration_thread_failed"))?
    }

    pub(crate) fn start(
        &self,
        request: AudioPrototypeStartRequest,
    ) -> Result<AudioPrototypeStatus, AudioPrototypeError> {
        let config = request.validate().map_err(AudioPrototypeError::new)?;
        let mut running = self
            .running
            .lock()
            .map_err(|_| AudioPrototypeError::new("audio_prototype_state_unavailable"))?;
        if running.is_some() {
            return Err(AudioPrototypeError::new("audio_prototype_already_running"));
        }
        let prototype = RunningAudioPrototype::start(config)?;
        let status = prototype.snapshot();
        *running = Some(prototype);
        Ok(status)
    }

    pub(crate) fn status(&self) -> Result<AudioPrototypeStatus, AudioPrototypeError> {
        let running = self
            .running
            .lock()
            .map_err(|_| AudioPrototypeError::new("audio_prototype_state_unavailable"))?;
        if let Some(prototype) = running.as_ref() {
            return Ok(prototype.snapshot());
        }
        self.last_status
            .lock()
            .map_err(|_| AudioPrototypeError::new("audio_prototype_state_unavailable"))?
            .clone()
            .ok_or_else(|| AudioPrototypeError::new("audio_prototype_not_started"))
    }

    pub(crate) fn stop(&self) -> Result<AudioPrototypeStatus, AudioPrototypeError> {
        let prototype = self
            .running
            .lock()
            .map_err(|_| AudioPrototypeError::new("audio_prototype_state_unavailable"))?
            .take()
            .ok_or_else(|| AudioPrototypeError::new("audio_prototype_not_running"))?;
        let status = prototype.stop();
        *self
            .last_status
            .lock()
            .map_err(|_| AudioPrototypeError::new("audio_prototype_state_unavailable"))? =
            Some(status.clone());
        Ok(status)
    }
}

pub(crate) struct RunningAudioPrototype {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<AudioPrototypeStatus>>,
    started: Instant,
    handles: Vec<JoinHandle<()>>,
}

impl RunningAudioPrototype {
    pub(crate) fn start(config: AudioPrototypeConfig) -> Result<Self, AudioPrototypeError> {
        if config.queue_capacity_packets_per_source == 0 {
            return Err(AudioPrototypeError::new("audio_queue_capacity_invalid"));
        }

        let started = Instant::now();
        let epoch = QpcEpoch::from_100ns(current_qpc_100ns()?);
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(AudioPrototypeStatus::new(
            config.queue_capacity_packets_per_source,
        )));

        let (microphone_sender, microphone_receiver) =
            packet_queue(config.queue_capacity_packets_per_source);
        let (system_sender, system_receiver) =
            packet_queue(config.queue_capacity_packets_per_source);
        let (microphone_processed_sender, microphone_processed_receiver) =
            bounded_queue(config.queue_capacity_packets_per_source);
        let (system_processed_sender, system_processed_receiver) =
            bounded_queue(config.queue_capacity_packets_per_source);

        let sink = spawn_processed_sink(
            Arc::clone(&status),
            microphone_processed_receiver,
            system_processed_receiver,
        )?;
        let microphone_processor = match spawn_processor(
            AudioSource::Microphone,
            Arc::clone(&status),
            microphone_receiver,
            microphone_processed_sender,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                stop.store(true, Ordering::Release);
                drop(microphone_sender);
                drop(system_sender);
                drop(system_processed_sender);
                let _ = sink.join();
                return Err(error);
            }
        };
        let system_processor = match spawn_processor(
            AudioSource::SystemOutput,
            Arc::clone(&status),
            system_receiver,
            system_processed_sender,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                stop.store(true, Ordering::Release);
                drop(microphone_sender);
                drop(system_sender);
                let _ = microphone_processor.join();
                let _ = sink.join();
                return Err(error);
            }
        };
        let microphone = match spawn_supervisor(
            AudioSource::Microphone,
            config.microphone,
            epoch,
            Arc::clone(&stop),
            Arc::clone(&status),
            microphone_sender,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                stop.store(true, Ordering::Release);
                drop(system_sender);
                let _ = microphone_processor.join();
                let _ = system_processor.join();
                let _ = sink.join();
                return Err(error);
            }
        };
        let system_output = match spawn_supervisor(
            AudioSource::SystemOutput,
            config.system_output,
            epoch,
            Arc::clone(&stop),
            Arc::clone(&status),
            system_sender,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                stop.store(true, Ordering::Release);
                let _ = microphone.join();
                let _ = microphone_processor.join();
                let _ = system_processor.join();
                let _ = sink.join();
                return Err(error);
            }
        };

        status.lock().expect("audio status lock poisoned").state = PrototypeRunState::Capturing;

        Ok(Self {
            stop,
            status,
            started,
            handles: vec![
                microphone,
                system_output,
                microphone_processor,
                system_processor,
                sink,
            ],
        })
    }

    pub(crate) fn snapshot(&self) -> AudioPrototypeStatus {
        let mut snapshot = self
            .status
            .lock()
            .expect("audio status lock poisoned")
            .clone();
        snapshot.elapsed_ms = duration_ms(self.started.elapsed());
        snapshot
    }

    pub(crate) fn stop(mut self) -> AudioPrototypeStatus {
        {
            let mut status = self.status.lock().expect("audio status lock poisoned");
            status.state = PrototypeRunState::Stopping;
        }
        self.stop.store(true, Ordering::Release);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
        let mut status = self.status.lock().expect("audio status lock poisoned");
        status.state = PrototypeRunState::Stopped;
        status.elapsed_ms = duration_ms(self.started.elapsed());
        status.microphone.status = ChannelStatus::Stopped;
        status.system_output.status = ChannelStatus::Stopped;
        status.clone()
    }
}

impl Drop for RunningAudioPrototype {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

pub(crate) fn list_audio_devices() -> Result<AudioDeviceList, AudioPrototypeError> {
    let _com = ComGuard::initialize()?;
    let enumerator = DeviceEnumerator::new().map_err(map_wasapi_error)?;

    let input_defaults = default_endpoint_ids(&enumerator, Direction::Capture);
    let output_defaults = default_endpoint_ids(&enumerator, Direction::Render);

    Ok(AudioDeviceList {
        inputs: enumerate_direction(
            &enumerator,
            Direction::Capture,
            AudioDirection::Input,
            &input_defaults,
        )?,
        outputs: enumerate_direction(
            &enumerator,
            Direction::Render,
            AudioDirection::Output,
            &output_defaults,
        )?,
    })
}

fn enumerate_direction(
    enumerator: &DeviceEnumerator,
    direction: Direction,
    public_direction: AudioDirection,
    defaults: &[Option<String>; 3],
) -> Result<Vec<AudioDevice>, AudioPrototypeError> {
    let collection = enumerator
        .get_device_collection(&direction)
        .map_err(map_wasapi_error)?;
    let device_count = collection.get_nbr_devices().map_err(map_wasapi_error)? as usize;
    if device_count > 256 {
        return Err(AudioPrototypeError::new("audio_device_count_exceeded"));
    }
    let mut devices = Vec::with_capacity(device_count);

    for device in &collection {
        let device = device.map_err(map_wasapi_error)?;
        let endpoint_id = device.get_id().map_err(map_wasapi_error)?;
        if !valid_endpoint_id(&endpoint_id) {
            return Err(AudioPrototypeError::new("audio_endpoint_id_invalid"));
        }
        let friendly_name =
            bounded_device_name(&device.get_friendlyname().map_err(map_wasapi_error)?);
        let native_format = device
            .get_iaudioclient()
            .and_then(|client| client.get_mixformat())
            .ok()
            .map(|format| native_format(&format));

        devices.push(AudioDevice {
            is_default_console: defaults[0].as_deref() == Some(endpoint_id.as_str()),
            is_default_multimedia: defaults[1].as_deref() == Some(endpoint_id.as_str()),
            is_default_communications: defaults[2].as_deref() == Some(endpoint_id.as_str()),
            endpoint_id,
            friendly_name,
            direction: public_direction,
            native_format,
        });
    }

    Ok(devices)
}

fn default_endpoint_ids(
    enumerator: &DeviceEnumerator,
    direction: Direction,
) -> [Option<String>; 3] {
    [Role::Console, Role::Multimedia, Role::Communications].map(|role| {
        enumerator
            .get_default_device_for_role(&direction, &role)
            .and_then(|device| device.get_id())
            .ok()
    })
}

fn spawn_supervisor(
    source: AudioSource,
    selection: DeviceSelection,
    epoch: QpcEpoch,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<AudioPrototypeStatus>>,
    sender: PacketSender,
) -> Result<JoinHandle<()>, AudioPrototypeError> {
    thread::Builder::new()
        .name(
            match source {
                AudioSource::Microphone => "audio-microphone-capture",
                AudioSource::SystemOutput => "audio-system-output-capture",
            }
            .to_owned(),
        )
        .spawn(move || supervise_channel(source, selection, epoch, stop, status, sender))
        .map_err(|_| AudioPrototypeError::new("audio_capture_thread_unavailable"))
}

fn supervise_channel(
    source: AudioSource,
    selection: DeviceSelection,
    epoch: QpcEpoch,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<AudioPrototypeStatus>>,
    sender: PacketSender,
) {
    let _com = match ComGuard::initialize() {
        Ok(com) => com,
        Err(error) => {
            record_failure(&status, source, error.code, ChannelStatus::Unavailable);
            return;
        }
    };

    supervise_attempts(source, &stop, &status, INITIAL_RETRY_DELAY, || {
        capture_until_stopped(source, &selection, epoch, &stop, &status, &sender)
    });
}

fn supervise_attempts(
    source: AudioSource,
    stop: &AtomicBool,
    status: &Mutex<AudioPrototypeStatus>,
    initial_retry_delay: Duration,
    mut capture_attempt: impl FnMut() -> Result<(), AudioPrototypeError>,
) {
    let mut retry_delay = initial_retry_delay;
    while !stop.load(Ordering::Acquire) {
        {
            let mut current = status.lock().expect("audio status lock poisoned");
            let channel = current.channel_mut(source);
            channel.capture_attempts = channel.capture_attempts.saturating_add(1);
            channel.status = if channel.capture_attempts == 1 {
                ChannelStatus::Starting
            } else {
                ChannelStatus::Reconnecting
            };
        }

        match capture_attempt() {
            Ok(()) => break,
            Err(error) => {
                record_failure(status, source, error.code, ChannelStatus::Reconnecting);
                if wait_for_stop(stop, retry_delay) {
                    break;
                }
                retry_delay = retry_delay.saturating_mul(2).min(MAX_RETRY_DELAY);
            }
        }
    }

    status
        .lock()
        .expect("audio status lock poisoned")
        .channel_mut(source)
        .status = ChannelStatus::Stopped;
}

fn capture_until_stopped(
    source: AudioSource,
    selection: &DeviceSelection,
    epoch: QpcEpoch,
    stop: &AtomicBool,
    status: &Mutex<AudioPrototypeStatus>,
    sender: &PacketSender,
) -> Result<(), AudioPrototypeError> {
    let enumerator = DeviceEnumerator::new().map_err(map_wasapi_error)?;
    let direction = match source {
        AudioSource::Microphone => Direction::Capture,
        AudioSource::SystemOutput => Direction::Render,
    };
    let device = resolve_device(&enumerator, direction, selection)?;
    let endpoint_id = device.get_id().map_err(map_wasapi_error)?;
    if !valid_endpoint_id(&endpoint_id) {
        return Err(AudioPrototypeError::new("audio_endpoint_id_invalid"));
    }
    let mut audio_client = device.get_iaudioclient().map_err(map_wasapi_error)?;
    let format = audio_client.get_mixformat().map_err(map_wasapi_error)?;
    let diagnostics_format = native_format(&format);
    let (_, minimum_period) = audio_client.get_device_period().map_err(map_wasapi_error)?;
    let mode = StreamMode::EventsShared {
        autoconvert: false,
        buffer_duration_hns: minimum_period,
    };
    audio_client
        .initialize_client(&format, &Direction::Capture, &mode)
        .map_err(map_wasapi_error)?;
    let event = audio_client
        .set_get_eventhandle()
        .map_err(map_wasapi_error)?;
    let capture = audio_client
        .get_audiocaptureclient()
        .map_err(map_wasapi_error)?;

    {
        let mut current = status.lock().expect("audio status lock poisoned");
        let channel = current.channel_mut(source);
        channel.status = ChannelStatus::Active;
        channel.endpoint_id = Some(endpoint_id.clone());
        channel.native_format = Some(diagnostics_format.clone());
        channel.last_error_code = None;
    }

    audio_client.start_stream().map_err(map_wasapi_error)?;
    let result = capture_event_loop(
        source,
        epoch,
        &diagnostics_format,
        &enumerator,
        direction,
        selection,
        &endpoint_id,
        stop,
        status,
        sender,
        &event,
        &capture,
    );
    let _ = audio_client.stop_stream();
    result
}

#[allow(clippy::too_many_arguments)]
fn capture_event_loop(
    source: AudioSource,
    epoch: QpcEpoch,
    native_format: &NativeAudioFormat,
    enumerator: &DeviceEnumerator,
    direction: Direction,
    selection: &DeviceSelection,
    endpoint_id: &str,
    stop: &AtomicBool,
    status: &Mutex<AudioPrototypeStatus>,
    sender: &PacketSender,
    event: &wasapi::Handle,
    capture: &wasapi::AudioCaptureClient,
) -> Result<(), AudioPrototypeError> {
    let block_align = native_format.block_align as usize;
    let mut next_default_check = Instant::now() + Duration::from_secs(1);
    while !stop.load(Ordering::Acquire) {
        match event.wait_for_event(EVENT_WAIT_MS) {
            Ok(()) | Err(WasapiError::EventTimeout) => {}
            Err(error) => return Err(map_wasapi_error(error)),
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if Instant::now() >= next_default_check {
            if default_endpoint_changed(enumerator, direction, selection, endpoint_id)? {
                return Err(AudioPrototypeError::new("audio_default_endpoint_changed"));
            }
            next_default_check = Instant::now() + Duration::from_secs(1);
        }

        loop {
            let frames = capture
                .get_next_packet_size()
                .map_err(map_wasapi_error)?
                .unwrap_or_default();
            if frames == 0 {
                break;
            }
            let byte_count = (frames as usize)
                .checked_mul(block_align)
                .ok_or_else(|| AudioPrototypeError::new("audio_packet_too_large"))?;
            let mut bytes = vec![0; byte_count];
            let (frames_read, info) = capture
                .read_from_device(&mut bytes)
                .map_err(map_wasapi_error)?;
            bytes.truncate(frames_read as usize * block_align);

            let timestamp_invalid = info.flags.timestamp_error;
            let qpc_100ns = if timestamp_invalid {
                current_qpc_100ns()?
            } else {
                info.timestamp
            };
            let start_ms = epoch.packet_ms(qpc_100ns);
            let packet = AudioPacket {
                source,
                start_ms,
                frames: frames_read,
                bytes,
                format: native_format.clone(),
            };
            let enqueue = sender.try_send(packet);

            let mut current = status.lock().expect("audio status lock poisoned");
            let channel = current.channel_mut(source);
            channel.packets_captured = channel.packets_captured.saturating_add(1);
            channel.frames_captured = channel.frames_captured.saturating_add(frames_read as u64);
            if info.flags.data_discontinuity {
                channel.data_discontinuities = channel.data_discontinuities.saturating_add(1);
            }
            if timestamp_invalid {
                channel.timestamp_errors = channel.timestamp_errors.saturating_add(1);
            }
            if channel
                .last_packet_ms
                .is_some_and(|previous| start_ms < previous)
            {
                channel.timestamp_regressions = channel.timestamp_regressions.saturating_add(1);
            }
            channel.first_packet_ms.get_or_insert(start_ms);
            channel.last_packet_ms = Some(start_ms);
            match enqueue {
                EnqueueResult::Enqueued => {}
                EnqueueResult::DroppedFull => {
                    channel.queue_drops = channel.queue_drops.saturating_add(1);
                }
                EnqueueResult::Disconnected => {
                    return Err(AudioPrototypeError::new("audio_packet_consumer_stopped"));
                }
            }
        }
    }
    Ok(())
}

fn spawn_processor(
    source: AudioSource,
    status: Arc<Mutex<AudioPrototypeStatus>>,
    receiver: PacketReceiver,
    processed: BoundedSender<ProcessedAudioChunk>,
) -> Result<JoinHandle<()>, AudioPrototypeError> {
    thread::Builder::new()
        .name(
            match source {
                AudioSource::Microphone => "audio-microphone-processing",
                AudioSource::SystemOutput => "audio-system-output-processing",
            }
            .to_owned(),
        )
        .spawn(move || process_packets(source, status, receiver, processed))
        .map_err(|_| AudioPrototypeError::new("audio_processing_thread_unavailable"))
}

fn process_packets(
    source: AudioSource,
    status: Arc<Mutex<AudioPrototypeStatus>>,
    receiver: PacketReceiver,
    processed: BoundedSender<ProcessedAudioChunk>,
) {
    let mut processor = SourceProcessor::new(source);
    loop {
        match receiver.receiver().try_recv() {
            Ok(packet) => {
                let frames = packet.frames as u64;
                let bytes = packet.bytes.len() as u64;
                let outcome = processor.process(packet);
                {
                    let mut current = status.lock().expect("audio status lock poisoned");
                    let channel = current.channel_mut(source);
                    channel.packets_consumed = channel.packets_consumed.saturating_add(1);
                    channel.frames_consumed = channel.frames_consumed.saturating_add(frames);
                    channel.bytes_consumed = channel.bytes_consumed.saturating_add(bytes);
                }

                match outcome {
                    Ok(outcome) => record_processing_outcome(&status, source, outcome, &processed),
                    Err(error) => {
                        let mut current = status.lock().expect("audio status lock poisoned");
                        let channel = current.channel_mut(source);
                        channel.processing_errors = channel.processing_errors.saturating_add(1);
                        channel.last_processing_error_code = Some(error.code.to_owned());
                    }
                }
            }
            Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(2)),
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

fn record_processing_outcome(
    status: &Mutex<AudioPrototypeStatus>,
    source: AudioSource,
    mut outcome: ProcessingOutcome,
    processed: &BoundedSender<ProcessedAudioChunk>,
) {
    let chunks_produced = outcome.chunks.len() as u64;
    let samples_produced = outcome
        .chunks
        .iter()
        .map(|chunk| chunk.samples.len() as u64)
        .sum::<u64>();
    let mut queue_drops = 0_u64;
    let mut queue_disconnected = false;
    for chunk in outcome.chunks.drain(..) {
        match processed.try_send(chunk) {
            EnqueueResult::Enqueued => {}
            EnqueueResult::DroppedFull => queue_drops = queue_drops.saturating_add(1),
            EnqueueResult::Disconnected => queue_disconnected = true,
        }
    }

    let mut current = status.lock().expect("audio status lock poisoned");
    let channel = current.channel_mut(source);
    channel.native_frames_decoded = channel
        .native_frames_decoded
        .saturating_add(outcome.native_frames_decoded);
    channel.normalized_chunks_produced = channel
        .normalized_chunks_produced
        .saturating_add(chunks_produced);
    channel.normalized_samples_produced = channel
        .normalized_samples_produced
        .saturating_add(samples_produced);
    channel.processing_queue_drops = channel.processing_queue_drops.saturating_add(queue_drops);
    channel.non_finite_samples_sanitized = channel
        .non_finite_samples_sanitized
        .saturating_add(outcome.non_finite_samples_sanitized);
    channel.format_changes = channel
        .format_changes
        .saturating_add(u64::from(outcome.format_changed));
    channel.resampler_delay_frames = outcome.resampler_delay_frames;
    channel.pending_native_frames = outcome.pending_native_frames;
    channel.pending_normalized_samples = outcome.pending_normalized_samples;
    channel.level_updates = channel
        .level_updates
        .saturating_add(outcome.level_updates.len() as u64);
    if let Some(latest) = outcome.level_updates.pop() {
        channel.latest_level = Some(latest);
    }
    if queue_disconnected {
        channel.processing_errors = channel.processing_errors.saturating_add(1);
        channel.last_processing_error_code = Some("audio_processing_consumer_stopped".to_owned());
    } else {
        channel.last_processing_error_code = None;
    }
}

fn spawn_processed_sink(
    status: Arc<Mutex<AudioPrototypeStatus>>,
    microphone: BoundedReceiver<ProcessedAudioChunk>,
    system_output: BoundedReceiver<ProcessedAudioChunk>,
) -> Result<JoinHandle<()>, AudioPrototypeError> {
    thread::Builder::new()
        .name("audio-normalized-sink".to_owned())
        .spawn(move || consume_processed_chunks(status, microphone, system_output))
        .map_err(|_| AudioPrototypeError::new("audio_processing_sink_unavailable"))
}

fn consume_processed_chunks(
    status: Arc<Mutex<AudioPrototypeStatus>>,
    microphone: BoundedReceiver<ProcessedAudioChunk>,
    system_output: BoundedReceiver<ProcessedAudioChunk>,
) {
    let mut microphone_disconnected = false;
    let mut system_disconnected = false;
    while !microphone_disconnected || !system_disconnected {
        let mut consumed_any = false;
        for (receiver, disconnected) in [
            (microphone.receiver(), &mut microphone_disconnected),
            (system_output.receiver(), &mut system_disconnected),
        ] {
            match receiver.try_recv() {
                Ok(chunk) => {
                    consumed_any = true;
                    let mut current = status.lock().expect("audio status lock poisoned");
                    let channel = current.channel_mut(chunk.source);
                    channel.normalized_chunks_consumed =
                        channel.normalized_chunks_consumed.saturating_add(1);
                    channel.normalized_samples_consumed = channel
                        .normalized_samples_consumed
                        .saturating_add(chunk.samples.len() as u64);
                    let _ = chunk.start_ms;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    *disconnected = true;
                }
            }
        }

        if !consumed_any && (!microphone_disconnected || !system_disconnected) {
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn resolve_device(
    enumerator: &DeviceEnumerator,
    direction: Direction,
    selection: &DeviceSelection,
) -> Result<Device, AudioPrototypeError> {
    match selection {
        DeviceSelection::Default { role } => enumerator
            .get_default_device_for_role(&direction, &wasapi_role(*role))
            .map_err(map_wasapi_error),
        DeviceSelection::Fixed { endpoint_id } => {
            enumerator.get_device(endpoint_id).map_err(map_wasapi_error)
        }
    }
}

fn default_endpoint_changed(
    enumerator: &DeviceEnumerator,
    direction: Direction,
    selection: &DeviceSelection,
    current_endpoint_id: &str,
) -> Result<bool, AudioPrototypeError> {
    let DeviceSelection::Default { role } = selection else {
        return Ok(false);
    };
    let resolved_id = enumerator
        .get_default_device_for_role(&direction, &wasapi_role(*role))
        .and_then(|device| device.get_id())
        .map_err(map_wasapi_error)?;
    Ok(current_endpoint_id != resolved_id)
}

fn valid_endpoint_id(endpoint_id: &str) -> bool {
    !endpoint_id.is_empty()
        && endpoint_id.encode_utf16().count() <= 1024
        && !endpoint_id.chars().any(char::is_control)
}

fn bounded_device_name(name: &str) -> String {
    let mut used_units = 0;
    let bounded: String = name
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take_while(|character| {
            let next_units = character.len_utf16();
            if used_units + next_units > 512 {
                false
            } else {
                used_units += next_units;
                true
            }
        })
        .collect();
    if bounded.is_empty() {
        "Unnamed audio endpoint".to_owned()
    } else {
        bounded
    }
}

fn wasapi_role(role: DeviceRole) -> Role {
    match role {
        DeviceRole::Console => Role::Console,
        DeviceRole::Multimedia => Role::Multimedia,
        DeviceRole::Communications => Role::Communications,
    }
}

fn native_format(format: &WaveFormat) -> NativeAudioFormat {
    let sample_type = match format.get_subformat() {
        Ok(SampleType::Float) => NativeSampleType::Float,
        Ok(SampleType::Int) => NativeSampleType::Integer,
        Err(_) => NativeSampleType::Unknown,
    };
    NativeAudioFormat {
        sample_rate: format.get_samplespersec(),
        channels: format.get_nchannels(),
        bits_per_sample: format.get_bitspersample(),
        valid_bits_per_sample: format.get_validbitspersample(),
        block_align: format.get_blockalign(),
        channel_mask: format.get_dwchannelmask(),
        sample_type,
    }
}

fn current_qpc_100ns() -> Result<u64, AudioPrototypeError> {
    let mut ticks = 0_i64;
    let mut frequency = 0_i64;
    let counter_ok = unsafe { QueryPerformanceCounter(&mut ticks) } != 0;
    let frequency_ok = unsafe { QueryPerformanceFrequency(&mut frequency) } != 0;
    if !counter_ok || !frequency_ok {
        return Err(AudioPrototypeError::new("audio_clock_unavailable"));
    }
    qpc_ticks_to_100ns(ticks, frequency)
        .ok_or_else(|| AudioPrototypeError::new("audio_clock_invalid"))
}

fn map_wasapi_error(error: WasapiError) -> AudioPrototypeError {
    let code = match error {
        WasapiError::DeviceNotFound(_) => "audio_endpoint_unavailable",
        WasapiError::EventTimeout => "audio_event_timeout",
        WasapiError::LoopbackWithExclusiveMode
        | WasapiError::RenderToCaptureDevice
        | WasapiError::IllegalDeviceDirection(_)
        | WasapiError::IllegalDeviceRole(_)
        | WasapiError::IllegalDeviceState(_) => "audio_capture_configuration_invalid",
        WasapiError::UnsupportedFormat | WasapiError::UnsupportedSubformat(_) => {
            "audio_native_format_unsupported"
        }
        _ => "audio_capture_stream_failed",
    };
    AudioPrototypeError::new(code)
}

fn record_failure(
    status: &Mutex<AudioPrototypeStatus>,
    source: AudioSource,
    code: &str,
    channel_status: ChannelStatus,
) {
    let mut current = status.lock().expect("audio status lock poisoned");
    let channel = current.channel_mut(source);
    channel.status = channel_status;
    channel.last_error_code = Some(code.to_owned());
}

fn wait_for_stop(stop: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if stop.load(Ordering::Acquire) {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    stop.load(Ordering::Acquire)
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

struct ComGuard;

impl ComGuard {
    fn initialize() -> Result<Self, AudioPrototypeError> {
        initialize_mta()
            .ok()
            .map_err(|_| AudioPrototypeError::new("audio_com_initialization_failed"))?;
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        deinitialize();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    use windows_sys::Win32::System::Diagnostics::Debug::Beep;

    use super::{
        AudioPrototypeConfig, AudioPrototypeError, AudioPrototypeStatus, AudioSource,
        ChannelStatus, RunningAudioPrototype, bounded_device_name, list_audio_devices,
        record_failure, supervise_attempts, valid_endpoint_id,
    };

    #[test]
    fn one_channel_failure_does_not_mutate_the_other_channel() {
        let status = Mutex::new(AudioPrototypeStatus::new(4));
        {
            let mut current = status.lock().unwrap();
            current.microphone.status = ChannelStatus::Active;
            current.system_output.status = ChannelStatus::Active;
        }

        record_failure(
            &status,
            AudioSource::Microphone,
            "audio_endpoint_unavailable",
            ChannelStatus::Reconnecting,
        );

        let current = status.lock().unwrap();
        assert_eq!(current.microphone.status, ChannelStatus::Reconnecting);
        assert_eq!(
            current.microphone.last_error_code.as_deref(),
            Some("audio_endpoint_unavailable")
        );
        assert_eq!(current.system_output.status, ChannelStatus::Active);
        assert_eq!(current.system_output.last_error_code, None);
    }

    #[test]
    fn selection_model_keeps_all_default_roles_and_fixed_ids_explicit() {
        use crate::audio::{DeviceRole, DeviceSelection};

        let selections = [
            DeviceSelection::Default {
                role: DeviceRole::Console,
            },
            DeviceSelection::Default {
                role: DeviceRole::Multimedia,
            },
            DeviceSelection::Default {
                role: DeviceRole::Communications,
            },
            DeviceSelection::Fixed {
                endpoint_id: "synthetic-endpoint-id".to_owned(),
            },
        ];

        assert_eq!(selections.len(), 4);
    }

    #[test]
    fn production_supervisor_retries_a_failed_attempt() {
        let stop = AtomicBool::new(false);
        let status = Mutex::new(AudioPrototypeStatus::new(4));
        let calls = AtomicUsize::new(0);

        supervise_attempts(
            AudioSource::Microphone,
            &stop,
            &status,
            Duration::from_millis(1),
            || {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    Err(AudioPrototypeError::new("injected_capture_failure"))
                } else {
                    stop.store(true, Ordering::Release);
                    Ok(())
                }
            },
        );

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let channel = &status.lock().unwrap().microphone;
        assert_eq!(channel.capture_attempts, 2);
        assert_eq!(channel.status, ChannelStatus::Stopped);
    }

    #[test]
    fn endpoint_diagnostics_are_bounded_and_control_free() {
        assert!(valid_endpoint_id("synthetic-endpoint"));
        assert!(!valid_endpoint_id("bad\nendpoint"));
        assert!(!valid_endpoint_id(&"x".repeat(1025)));
        assert_eq!(bounded_device_name("  Device\nName  "), "DeviceName");
        assert_eq!(bounded_device_name("\n\r"), "Unnamed audio endpoint");
        assert!(
            bounded_device_name(&"ðŸŽ¤".repeat(300))
                .encode_utf16()
                .count()
                <= 512
        );
    }

    #[test]
    #[ignore = "requires active Windows microphone and render endpoints"]
    fn hardware_probe_enumerates_and_captures_both_default_endpoints() {
        let status = capture_hardware_status();

        assert!(status.microphone.native_format.is_some());
        assert!(status.system_output.native_format.is_some());
        assert!(status.microphone.packets_captured > 0);
        assert!(status.system_output.packets_captured > 0);
        assert_eq!(status.microphone.timestamp_regressions, 0);
        assert_eq!(status.system_output.timestamp_regressions, 0);
        assert_eq!(status.microphone.queue_drops, 0);
        assert_eq!(status.system_output.queue_drops, 0);
        assert_eq!(
            status.microphone.packets_captured,
            status.microphone.packets_consumed
        );
        assert_eq!(
            status.system_output.packets_captured,
            status.system_output.packets_consumed
        );
    }

    #[test]
    #[ignore = "requires active Windows microphone and render endpoints"]
    fn hardware_probe_processes_both_default_sources_to_16khz_mono() {
        let status = capture_hardware_status();

        for channel in [&status.microphone, &status.system_output] {
            assert!(channel.native_frames_decoded > 0);
            assert!(channel.normalized_chunks_produced > 0);
            assert_eq!(
                channel.normalized_samples_produced,
                channel.normalized_chunks_produced * 160
            );
            assert_eq!(
                channel.normalized_chunks_produced,
                channel.normalized_chunks_consumed + channel.processing_queue_drops
            );
            assert_eq!(channel.processing_queue_drops, 0);
            assert_eq!(channel.processing_errors, 0);
            assert_eq!(channel.non_finite_samples_sanitized, 0);
            assert_eq!(channel.last_processing_error_code, None);
            assert!(channel.level_updates > 0);
            let level = channel.latest_level.as_ref().expect("latest level");
            assert!(level.rms_dbfs.is_finite());
            assert!((-120.0..=0.0).contains(&level.rms_dbfs));
            assert!(level.peak_dbfs.is_finite());
            assert!((-120.0..=0.0).contains(&level.peak_dbfs));
        }
    }

    fn capture_hardware_status() -> AudioPrototypeStatus {
        let devices = list_audio_devices().expect("active endpoints should enumerate");
        assert!(
            !devices.inputs.is_empty(),
            "an active input endpoint is required"
        );
        assert!(
            !devices.outputs.is_empty(),
            "an active output endpoint is required"
        );
        assert!(
            devices
                .inputs
                .iter()
                .any(|device| device.is_default_console)
        );
        assert!(
            devices
                .outputs
                .iter()
                .any(|device| device.is_default_console)
        );

        let running = RunningAudioPrototype::start(AudioPrototypeConfig::default())
            .expect("dual capture should start");
        thread::sleep(Duration::from_secs(1));
        unsafe {
            Beep(880, 750);
        }
        thread::sleep(Duration::from_secs(4));
        let live_status = running.snapshot();
        assert_eq!(live_status.state, super::PrototypeRunState::Capturing);
        let status = running.stop();
        eprintln!("{}", serde_json::to_string_pretty(&status).unwrap());
        status
    }
}
