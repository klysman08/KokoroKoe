use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::audio::{AudioSource, DeviceSelection, DeviceTestInput};

use super::{AppError, RequestId, now_rfc3339};

pub(crate) const MAX_LIVE_TRANSCRIPT_BYTES: usize = 1_048_576;
pub(crate) const MAX_LIVE_LANGUAGE_BYTES: usize = 64;
pub(crate) const MAX_LIVE_GAP_CODE_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveTranscriptionInput {
    pub(crate) acknowledged_capture_consent: bool,
    pub(crate) microphone_selection: DeviceSelection,
    pub(crate) system_output_selection: DeviceSelection,
}

impl LiveTranscriptionInput {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !self.acknowledged_capture_consent {
            return Err("live_transcription_consent_required");
        }
        DeviceTestInput {
            source: AudioSource::Microphone,
            selection: self.microphone_selection.clone(),
        }
        .validate()?;
        DeviceTestInput {
            source: AudioSource::SystemOutput,
            selection: self.system_output_selection.clone(),
        }
        .validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LiveTranscriptionRunState {
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveTranscriptionStatus {
    pub(crate) state: LiveTranscriptionRunState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stopped_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<AppError>,
}

impl Default for LiveTranscriptionStatus {
    fn default() -> Self {
        Self {
            state: LiveTranscriptionRunState::Idle,
            request_id: None,
            started_at: None,
            stopped_at: None,
            error: None,
        }
    }
}

impl LiveTranscriptionStatus {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let timestamp_valid = |value: &Option<String>| {
            value
                .as_ref()
                .is_none_or(|value| OffsetDateTime::parse(value, &Rfc3339).is_ok())
        };
        if !timestamp_valid(&self.started_at) || !timestamp_valid(&self.stopped_at) {
            return Err("live_transcription_status_invalid");
        }
        match self.state {
            LiveTranscriptionRunState::Idle => {
                if self.request_id.is_some()
                    || self.started_at.is_some()
                    || self.stopped_at.is_some()
                    || self.error.is_some()
                {
                    return Err("live_transcription_status_invalid");
                }
            }
            LiveTranscriptionRunState::Starting
            | LiveTranscriptionRunState::Running
            | LiveTranscriptionRunState::Stopping => {
                if self.request_id.is_none()
                    || self.started_at.is_none()
                    || self.stopped_at.is_some()
                    || self.error.is_some()
                {
                    return Err("live_transcription_status_invalid");
                }
            }
            LiveTranscriptionRunState::Stopped => {
                if self.request_id.is_none()
                    || self.started_at.is_none()
                    || self.stopped_at.is_none()
                    || self.error.is_some()
                {
                    return Err("live_transcription_status_invalid");
                }
            }
            LiveTranscriptionRunState::Failed => {
                if self.request_id.is_none()
                    || self.started_at.is_none()
                    || self.stopped_at.is_none()
                    || self.error.is_none()
                {
                    return Err("live_transcription_status_invalid");
                }
            }
        }
        Ok(())
    }

    pub(crate) fn starting(request_id: RequestId) -> Result<Self, AppError> {
        Ok(Self {
            state: LiveTranscriptionRunState::Starting,
            request_id: Some(request_id),
            started_at: Some(now_rfc3339()?),
            stopped_at: None,
            error: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LiveSegmentStatus {
    Partial,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveTranscriptSegment {
    pub(crate) id: Uuid,
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) text: String,
    pub(crate) status: LiveSegmentStatus,
    pub(crate) language: String,
}

impl LiveTranscriptSegment {
    fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_nil()
            || self.end_ms <= self.start_ms
            || self.end_ms > 9_007_199_254_740_991
            || self.text.len() > MAX_LIVE_TRANSCRIPT_BYTES
            || self.text.chars().any(|value| value == '\0')
            || self.language.is_empty()
            || self.language.len() > MAX_LIVE_LANGUAGE_BYTES
            || self.language.chars().any(char::is_control)
        {
            return Err("live_transcript_segment_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptionPartialPayload {
    pub(crate) segment: LiveTranscriptSegment,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptionFinalPayload {
    pub(crate) segment: LiveTranscriptSegment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) replaces_partial_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptionGapPayload {
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) code: String,
}

pub(crate) trait LiveEventPayload {
    fn validate(&self) -> Result<(), &'static str>;
}

impl LiveEventPayload for TranscriptionPartialPayload {
    fn validate(&self) -> Result<(), &'static str> {
        self.segment.validate()?;
        if self.segment.status != LiveSegmentStatus::Partial {
            return Err("live_transcript_partial_invalid");
        }
        Ok(())
    }
}

impl LiveEventPayload for TranscriptionFinalPayload {
    fn validate(&self) -> Result<(), &'static str> {
        self.segment.validate()?;
        if self.segment.status != LiveSegmentStatus::Final
            || self
                .replaces_partial_id
                .is_some_and(|value| value != self.segment.id)
        {
            return Err("live_transcript_final_invalid");
        }
        Ok(())
    }
}

impl LiveEventPayload for TranscriptionGapPayload {
    fn validate(&self) -> Result<(), &'static str> {
        if self.end_ms <= self.start_ms
            || self.end_ms > 9_007_199_254_740_991
            || self.code.is_empty()
            || self.code.len() > MAX_LIVE_GAP_CODE_BYTES
            || !self
                .code
                .chars()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '_')
        {
            return Err("live_transcription_gap_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveEventEnvelope<T> {
    pub(crate) schema_version: u8,
    pub(crate) event_id: Uuid,
    pub(crate) emitted_at: String,
    pub(crate) session_sequence: u64,
    pub(crate) request_id: RequestId,
    pub(crate) payload: T,
}

impl<T: LiveEventPayload> LiveEventEnvelope<T> {
    pub(crate) fn new(
        request_id: RequestId,
        session_sequence: u64,
        payload: T,
    ) -> Result<Self, AppError> {
        let event = Self {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            emitted_at: now_rfc3339()?,
            session_sequence,
            request_id,
            payload,
        };
        event
            .validate()
            .map_err(AppError::live_transcription_error)?;
        Ok(event)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.event_id.is_nil()
            || self.session_sequence == 0
            || self.session_sequence > 9_007_199_254_740_991
            || OffsetDateTime::parse(&self.emitted_at, &Rfc3339).is_err()
        {
            return Err("live_transcription_event_invalid");
        }
        self.payload.validate()
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Fixture {
        input: LiveTranscriptionInput,
        idle_status: LiveTranscriptionStatus,
        running_status: LiveTranscriptionStatus,
        stopped_status: LiveTranscriptionStatus,
        partial_event: LiveEventEnvelope<TranscriptionPartialPayload>,
        final_event: LiveEventEnvelope<TranscriptionFinalPayload>,
        gap_event: LiveEventEnvelope<TranscriptionGapPayload>,
    }

    #[test]
    fn live_transcription_contract_matches_shared_fixture() {
        let fixture = include_str!("../../../fixtures/contracts/live-transcription-v1.json");
        let parsed: Fixture = serde_json::from_str(fixture).unwrap();
        assert!(parsed.input.validate().is_ok());
        assert!(parsed.idle_status.validate().is_ok());
        assert!(parsed.running_status.validate().is_ok());
        assert!(parsed.stopped_status.validate().is_ok());
        assert!(parsed.partial_event.validate().is_ok());
        assert!(parsed.final_event.validate().is_ok());
        assert!(parsed.gap_event.validate().is_ok());
        assert_eq!(
            parsed.partial_event.payload.segment.id,
            parsed.final_event.payload.segment.id
        );
        assert_eq!(
            parsed.final_event.payload.replaces_partial_id,
            Some(parsed.partial_event.payload.segment.id)
        );
        assert!(
            parsed.partial_event.session_sequence < parsed.final_event.session_sequence
                && parsed.final_event.session_sequence < parsed.gap_event.session_sequence
        );
    }

    #[test]
    fn live_transcription_contract_rejects_unknown_null_and_inconsistent_values() {
        let fixture = include_str!("../../../fixtures/contracts/live-transcription-v1.json");
        let base: serde_json::Value = serde_json::from_str(fixture).unwrap();
        for mutation in [
            ("unknown", serde_json::json!(true)),
            ("null", serde_json::Value::Null),
        ] {
            let mut invalid = base.clone();
            invalid["partialEvent"][mutation.0] = mutation.1;
            assert!(serde_json::from_value::<Fixture>(invalid).is_err());
        }

        let mut wrong_status = base.clone();
        wrong_status["partialEvent"]["payload"]["segment"]["status"] = serde_json::json!("final");
        let parsed: Fixture = serde_json::from_value(wrong_status).unwrap();
        assert!(parsed.partial_event.validate().is_err());

        let mut wrong_replacement = base;
        wrong_replacement["finalEvent"]["payload"]["replacesPartialId"] =
            serde_json::json!("34ad1c4d-3c82-4f4e-aa7c-e555b667f2cd");
        let parsed: Fixture = serde_json::from_value(wrong_replacement).unwrap();
        assert!(parsed.final_event.validate().is_err());
    }
}
