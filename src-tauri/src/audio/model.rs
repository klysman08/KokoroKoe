use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use super::UtteranceEndReason;
use crate::domain::{AppError, RequestId, now_rfc3339};

pub(crate) const DEFAULT_PACKET_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AudioSource {
    Microphone,
    SystemOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AudioDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AudioDeviceState {
    Active,
    Disabled,
    NotPresent,
    Unplugged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeviceRole {
    Console,
    Multimedia,
    Communications,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum DeviceSelection {
    Default { role: DeviceRole },
    Fixed { endpoint_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativeSampleType {
    Float,
    Integer,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeAudioFormat {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) bits_per_sample: u16,
    pub(crate) valid_bits_per_sample: u16,
    pub(crate) block_align: u32,
    pub(crate) channel_mask: u32,
    pub(crate) sample_type: NativeSampleType,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioDevice {
    pub(crate) endpoint_id: String,
    pub(crate) friendly_name: String,
    pub(crate) direction: AudioDirection,
    pub(crate) state: AudioDeviceState,
    pub(crate) is_default_console: bool,
    pub(crate) is_default_multimedia: bool,
    pub(crate) is_default_communications: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channels: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioDeviceList {
    pub(crate) inputs: Vec<AudioDevice>,
    pub(crate) outputs: Vec<AudioDevice>,
}

impl AudioDeviceList {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.inputs.len() > 256 || self.outputs.len() > 256 {
            return Err("audio_device_count_exceeded");
        }
        for device in &self.inputs {
            device.validate(AudioDirection::Input)?;
        }
        for device in &self.outputs {
            device.validate(AudioDirection::Output)?;
        }
        Ok(())
    }
}

impl AudioDevice {
    fn validate(&self, expected_direction: AudioDirection) -> Result<(), &'static str> {
        if self.direction != expected_direction
            || !valid_endpoint_id(&self.endpoint_id)
            || self.friendly_name.is_empty()
            || self.friendly_name.encode_utf16().count() > 512
            || self.friendly_name.chars().any(char::is_control)
            || self.sample_rate == Some(0)
            || self.channels == Some(0)
        {
            return Err("audio_device_contract_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeviceTestInput {
    pub(crate) source: AudioSource,
    pub(crate) selection: DeviceSelection,
}

impl DeviceTestInput {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if let DeviceSelection::Fixed { endpoint_id } = &self.selection
            && !valid_endpoint_id(endpoint_id)
        {
            return Err("audio_endpoint_id_invalid");
        }
        Ok(())
    }
}

fn valid_endpoint_id(endpoint_id: &str) -> bool {
    !endpoint_id.is_empty()
        && endpoint_id.encode_utf16().count() <= 1024
        && !endpoint_id.chars().any(char::is_control)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeviceTestRunStatus {
    Starting,
    Active,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeviceTestStatus {
    pub(crate) request_id: RequestId,
    pub(crate) source: AudioSource,
    pub(crate) status: DeviceTestRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) device: Option<AudioDevice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<AppError>,
}

impl DeviceTestStatus {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if (self.status == DeviceTestRunStatus::Failed) != self.error.is_some() {
            return Err("audio_device_test_status_invalid");
        }
        if let Some(device) = &self.device {
            let direction = match self.source {
                AudioSource::Microphone => AudioDirection::Input,
                AudioSource::SystemOutput => AudioDirection::Output,
            };
            device.validate(direction)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChannelHealthStatus {
    Starting,
    Active,
    Silent,
    Reconnecting,
    Unavailable,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChannelHealth {
    pub(crate) status: ChannelHealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail_code: Option<String>,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioLevelUpdated {
    pub(crate) test_id: RequestId,
    pub(crate) source: AudioSource,
    pub(crate) rms_dbfs: f64,
    pub(crate) peak_dbfs: f64,
    pub(crate) clipping: bool,
    pub(crate) muted: bool,
    pub(crate) at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioDeviceStatusChanged {
    pub(crate) source: AudioSource,
    pub(crate) previous: ChannelHealth,
    pub(crate) current: ChannelHealth,
    pub(crate) is_default_change: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioEventEnvelope<T> {
    pub(crate) schema_version: u8,
    pub(crate) event_id: Uuid,
    pub(crate) emitted_at: String,
    pub(crate) request_id: RequestId,
    pub(crate) payload: T,
}

pub(crate) trait AudioPayloadContract {
    fn validate(&self, request_id: RequestId) -> Result<(), &'static str>;
}

impl AudioPayloadContract for AudioLevelUpdated {
    fn validate(&self, request_id: RequestId) -> Result<(), &'static str> {
        if self.test_id != request_id
            || !self.rms_dbfs.is_finite()
            || !self.peak_dbfs.is_finite()
            || !(-120.0..=0.0).contains(&self.rms_dbfs)
            || !(-120.0..=0.0).contains(&self.peak_dbfs)
            || self.rms_dbfs > self.peak_dbfs
            || self.at_ms > 9_007_199_254_740_991
        {
            return Err("audio_level_event_invalid");
        }
        Ok(())
    }
}

impl AudioPayloadContract for AudioDeviceStatusChanged {
    fn validate(&self, _request_id: RequestId) -> Result<(), &'static str> {
        validate_channel_health(&self.previous)?;
        validate_channel_health(&self.current)
    }
}

fn validate_channel_health(health: &ChannelHealth) -> Result<(), &'static str> {
    if OffsetDateTime::parse(&health.updated_at, &Rfc3339).is_err()
        || health
            .endpoint_id
            .as_ref()
            .is_some_and(|value| !valid_endpoint_id(value))
        || health.detail_code.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.encode_utf16().count() > 128
                || value.chars().any(char::is_control)
        })
    {
        return Err("audio_health_event_invalid");
    }
    Ok(())
}

impl<T: AudioPayloadContract> AudioEventEnvelope<T> {
    pub(crate) fn new(request_id: RequestId, payload: T) -> Result<Self, AppError> {
        let event = Self {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            emitted_at: now_rfc3339()?,
            request_id,
            payload,
        };
        event.validate().map_err(AppError::audio_operation_failed)?;
        Ok(event)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 || OffsetDateTime::parse(&self.emitted_at, &Rfc3339).is_err() {
            return Err("audio_event_envelope_invalid");
        }
        self.payload.validate(self.request_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrototypeRunState {
    Starting,
    Capturing,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChannelStatus {
    Starting,
    Active,
    Reconnecting,
    Unavailable,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LevelDiagnostics {
    pub(crate) rms_dbfs: f64,
    pub(crate) peak_dbfs: f64,
    pub(crate) clipping: bool,
    pub(crate) at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UtteranceDiagnostics {
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) duration_ms: u64,
    pub(crate) end_reason: UtteranceEndReason,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChannelDiagnostics {
    pub(crate) source: AudioSource,
    pub(crate) status: ChannelStatus,
    pub(crate) endpoint_id: Option<String>,
    pub(crate) native_format: Option<NativeAudioFormat>,
    pub(crate) capture_attempts: u64,
    pub(crate) capture_failures: u64,
    pub(crate) recovery_gaps: u64,
    pub(crate) recovery_gap_ms: u64,
    pub(crate) recovery_pending_since_ms: Option<u64>,
    pub(crate) last_recovery_gap_ms: Option<u64>,
    pub(crate) last_capture_failure_code: Option<String>,
    pub(crate) packets_captured: u64,
    pub(crate) frames_captured: u64,
    pub(crate) packets_consumed: u64,
    pub(crate) frames_consumed: u64,
    pub(crate) bytes_consumed: u64,
    pub(crate) queue_drops: u64,
    pub(crate) native_frames_decoded: u64,
    pub(crate) normalized_chunks_produced: u64,
    pub(crate) normalized_samples_produced: u64,
    pub(crate) normalized_chunks_consumed: u64,
    pub(crate) normalized_samples_consumed: u64,
    pub(crate) processing_queue_drops: u64,
    pub(crate) processing_errors: u64,
    pub(crate) non_finite_samples_sanitized: u64,
    pub(crate) format_changes: u64,
    pub(crate) resampler_delay_frames: u64,
    pub(crate) pending_native_frames: u64,
    pub(crate) pending_normalized_samples: u64,
    pub(crate) level_updates: u64,
    pub(crate) latest_level: Option<LevelDiagnostics>,
    pub(crate) vad_frames_analyzed: u64,
    pub(crate) vad_speech_frames: u64,
    pub(crate) vad_silence_frames: u64,
    pub(crate) utterances_finalized: u64,
    pub(crate) utterance_samples_finalized: u64,
    pub(crate) short_utterances_rejected: u64,
    pub(crate) forced_splits: u64,
    pub(crate) vad_resets: u64,
    pub(crate) vad_pending_samples: u64,
    pub(crate) vad_buffered_samples: u64,
    pub(crate) latest_utterance: Option<UtteranceDiagnostics>,
    pub(crate) data_discontinuities: u64,
    pub(crate) timestamp_errors: u64,
    pub(crate) timestamp_regressions: u64,
    pub(crate) first_packet_ms: Option<u64>,
    pub(crate) last_packet_ms: Option<u64>,
    pub(crate) last_error_code: Option<String>,
    pub(crate) last_processing_error_code: Option<String>,
}

impl ChannelDiagnostics {
    pub(crate) fn starting(source: AudioSource) -> Self {
        Self {
            source,
            status: ChannelStatus::Starting,
            endpoint_id: None,
            native_format: None,
            capture_attempts: 0,
            capture_failures: 0,
            recovery_gaps: 0,
            recovery_gap_ms: 0,
            recovery_pending_since_ms: None,
            last_recovery_gap_ms: None,
            last_capture_failure_code: None,
            packets_captured: 0,
            frames_captured: 0,
            packets_consumed: 0,
            frames_consumed: 0,
            bytes_consumed: 0,
            queue_drops: 0,
            native_frames_decoded: 0,
            normalized_chunks_produced: 0,
            normalized_samples_produced: 0,
            normalized_chunks_consumed: 0,
            normalized_samples_consumed: 0,
            processing_queue_drops: 0,
            processing_errors: 0,
            non_finite_samples_sanitized: 0,
            format_changes: 0,
            resampler_delay_frames: 0,
            pending_native_frames: 0,
            pending_normalized_samples: 0,
            level_updates: 0,
            latest_level: None,
            vad_frames_analyzed: 0,
            vad_speech_frames: 0,
            vad_silence_frames: 0,
            utterances_finalized: 0,
            utterance_samples_finalized: 0,
            short_utterances_rejected: 0,
            forced_splits: 0,
            vad_resets: 0,
            vad_pending_samples: 0,
            vad_buffered_samples: 0,
            latest_utterance: None,
            data_discontinuities: 0,
            timestamp_errors: 0,
            timestamp_regressions: 0,
            first_packet_ms: None,
            last_packet_ms: None,
            last_error_code: None,
            last_processing_error_code: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioPrototypeStatus {
    pub(crate) state: PrototypeRunState,
    pub(crate) elapsed_ms: u64,
    pub(crate) queue_capacity_packets_per_source: usize,
    pub(crate) processing_queue_capacity_chunks_per_source: usize,
    pub(crate) microphone: ChannelDiagnostics,
    pub(crate) system_output: ChannelDiagnostics,
}

impl AudioPrototypeStatus {
    pub(crate) fn new(queue_capacity_packets_per_source: usize) -> Self {
        Self {
            state: PrototypeRunState::Starting,
            elapsed_ms: 0,
            queue_capacity_packets_per_source,
            processing_queue_capacity_chunks_per_source: queue_capacity_packets_per_source,
            microphone: ChannelDiagnostics::starting(AudioSource::Microphone),
            system_output: ChannelDiagnostics::starting(AudioSource::SystemOutput),
        }
    }

    pub(crate) fn channel_mut(&mut self, source: AudioSource) -> &mut ChannelDiagnostics {
        match source {
            AudioSource::Microphone => &mut self.microphone,
            AudioSource::SystemOutput => &mut self.system_output,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AudioPrototypeConfig {
    pub(crate) microphone: DeviceSelection,
    pub(crate) system_output: DeviceSelection,
    pub(crate) queue_capacity_packets_per_source: usize,
}

impl Default for AudioPrototypeConfig {
    fn default() -> Self {
        Self {
            microphone: DeviceSelection::Default {
                role: DeviceRole::Console,
            },
            system_output: DeviceSelection::Default {
                role: DeviceRole::Console,
            },
            queue_capacity_packets_per_source: DEFAULT_PACKET_QUEUE_CAPACITY,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioPrototypeStartRequest {
    pub(crate) acknowledged_capture_consent: bool,
    pub(crate) microphone: DeviceSelection,
    pub(crate) system_output: DeviceSelection,
    pub(crate) queue_capacity_packets_per_source: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
impl AudioPrototypeStartRequest {
    pub(crate) fn validate(self) -> Result<AudioPrototypeConfig, &'static str> {
        if !self.acknowledged_capture_consent {
            return Err("audio_capture_consent_required");
        }
        if !(4..=256).contains(&self.queue_capacity_packets_per_source) {
            return Err("audio_queue_capacity_invalid");
        }
        for selection in [&self.microphone, &self.system_output] {
            if let DeviceSelection::Fixed { endpoint_id } = selection
                && (endpoint_id.is_empty()
                    || endpoint_id.encode_utf16().count() > 1024
                    || endpoint_id.chars().any(char::is_control))
            {
                return Err("audio_endpoint_id_invalid");
            }
        }
        Ok(AudioPrototypeConfig {
            microphone: self.microphone,
            system_output: self.system_output,
            queue_capacity_packets_per_source: self.queue_capacity_packets_per_source,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::{
        AudioDeviceList, AudioDeviceStatusChanged, AudioEventEnvelope, AudioLevelUpdated,
        AudioPrototypeStartRequest, AudioPrototypeStatus, DeviceRole, DeviceSelection,
        DeviceTestInput, DeviceTestStatus,
    };

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ProductAudioFixture {
        device_list: AudioDeviceList,
        input: DeviceTestInput,
        status: DeviceTestStatus,
        level_event: AudioEventEnvelope<AudioLevelUpdated>,
        health_event: AudioEventEnvelope<AudioDeviceStatusChanged>,
    }

    #[test]
    fn product_audio_contract_matches_the_shared_fixture() {
        let fixture = include_str!("../../../fixtures/contracts/audio-device-test-v1.json");
        let parsed: ProductAudioFixture =
            serde_json::from_str(fixture).expect("shared product audio fixture should parse");
        assert_eq!(parsed.device_list.inputs.len(), 1);
        assert!(parsed.device_list.validate().is_ok());
        assert!(parsed.input.validate().is_ok());
        assert!(parsed.status.validate().is_ok());
        assert!(parsed.level_event.validate().is_ok());
        assert!(parsed.health_event.validate().is_ok());
        assert_eq!(parsed.status.request_id, parsed.level_event.request_id);
        assert_eq!(parsed.status.request_id, parsed.level_event.payload.test_id);
        assert_eq!(parsed.status.request_id, parsed.health_event.request_id);
        assert_eq!(parsed.status.source, parsed.level_event.payload.source);
        assert_eq!(parsed.status.source, parsed.health_event.payload.source);

        let unknown = fixture.replacen(
            "\"source\": \"microphone\",",
            "\"source\": \"microphone\", \"samples\": [],",
            1,
        );
        assert!(serde_json::from_str::<ProductAudioFixture>(&unknown).is_err());
    }

    #[test]
    fn prototype_start_requires_consent_and_bounded_queues() {
        let fixture = include_str!("../../../fixtures/contracts/audio-prototype-start-v1.json");
        let parsed: AudioPrototypeStartRequest =
            serde_json::from_str(fixture).expect("shared audio start fixture should parse");
        assert!(parsed.validate().is_ok());

        let request = AudioPrototypeStartRequest {
            acknowledged_capture_consent: false,
            microphone: DeviceSelection::Default {
                role: DeviceRole::Console,
            },
            system_output: DeviceSelection::Fixed {
                endpoint_id: "synthetic-output".to_owned(),
            },
            queue_capacity_packets_per_source: 64,
        };
        assert_eq!(
            request.clone().validate(),
            Err("audio_capture_consent_required")
        );

        let invalid_capacity = AudioPrototypeStartRequest {
            acknowledged_capture_consent: true,
            queue_capacity_packets_per_source: 257,
            ..request
        };
        assert_eq!(
            invalid_capacity.validate(),
            Err("audio_queue_capacity_invalid")
        );
    }

    #[test]
    fn prototype_start_rejects_unknown_fields_and_malformed_fixed_ids() {
        let unknown = serde_json::json!({
            "acknowledgedCaptureConsent": true,
            "microphone": { "kind": "default", "role": "console" },
            "systemOutput": { "kind": "fixed", "endpointId": "output" },
            "queueCapacityPacketsPerSource": 64,
            "extra": true
        });
        assert!(serde_json::from_value::<AudioPrototypeStartRequest>(unknown).is_err());

        let request = AudioPrototypeStartRequest {
            acknowledged_capture_consent: true,
            microphone: DeviceSelection::Default {
                role: DeviceRole::Multimedia,
            },
            system_output: DeviceSelection::Fixed {
                endpoint_id: "bad\nendpoint".to_owned(),
            },
            queue_capacity_packets_per_source: 64,
        };
        assert_eq!(request.validate(), Err("audio_endpoint_id_invalid"));
    }

    #[test]
    fn normalized_processing_status_matches_the_shared_contract_fixture() {
        let fixture = include_str!("../../../fixtures/contracts/audio-prototype-status-v1.json");
        let parsed: AudioPrototypeStatus =
            serde_json::from_str(fixture).expect("shared audio status fixture should parse");

        assert_eq!(parsed.processing_queue_capacity_chunks_per_source, 64);
        assert_eq!(parsed.microphone.capture_failures, 1);
        assert_eq!(parsed.microphone.recovery_gaps, 1);
        assert_eq!(parsed.microphone.last_recovery_gap_ms, Some(260));
        assert_eq!(parsed.microphone.normalized_samples_produced, 18_880);
        assert_eq!(parsed.system_output.native_frames_decoded, 52_920);
        assert_eq!(
            parsed
                .microphone
                .latest_level
                .expect("microphone level")
                .at_ms,
            1_100
        );

        let unknown = fixture.replacen(
            "\"processingQueueCapacityChunksPerSource\": 64,",
            "\"processingQueueCapacityChunksPerSource\": 64, \"rawSamples\": [],",
            1,
        );
        assert!(serde_json::from_str::<AudioPrototypeStatus>(&unknown).is_err());
    }
}
