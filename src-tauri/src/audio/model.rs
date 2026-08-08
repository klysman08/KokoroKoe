use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_PACKET_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
    pub(crate) is_default_console: bool,
    pub(crate) is_default_multimedia: bool,
    pub(crate) is_default_communications: bool,
    pub(crate) native_format: Option<NativeAudioFormat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioDeviceList {
    pub(crate) inputs: Vec<AudioDevice>,
    pub(crate) outputs: Vec<AudioDevice>,
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChannelDiagnostics {
    pub(crate) source: AudioSource,
    pub(crate) status: ChannelStatus,
    pub(crate) endpoint_id: Option<String>,
    pub(crate) native_format: Option<NativeAudioFormat>,
    pub(crate) capture_attempts: u64,
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
    use super::{AudioPrototypeStartRequest, AudioPrototypeStatus, DeviceRole, DeviceSelection};

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
