use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audio::AudioSource;

use super::{ProjectId, SessionId};

const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;
pub(crate) const MAX_TRANSCRIPT_PAGE_LIMIT: u16 = 100;
pub(crate) const MAX_TRANSCRIPT_QUERY_BYTES: usize = 256;
pub(crate) const MAX_TRANSCRIPT_QUERY_TERMS: usize = 16;
pub(crate) const MAX_TRANSCRIPT_TEXT_BYTES: usize = 32 * 1024;
pub(crate) const MAX_TRANSCRIPT_SNIPPET_CHARS: usize = 240;
pub(crate) const MAX_TRANSCRIPT_LANGUAGE_BYTES: usize = 64;
pub(crate) const MAX_TRANSCRIPT_CURSOR_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TranscriptSegmentStatus {
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptSegmentView {
    pub(crate) id: Uuid,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) text: String,
    pub(crate) status: TranscriptSegmentStatus,
    pub(crate) language: String,
    /// The transcription as it was produced, present only when the user
    /// rewrote this segment. A correction never replaces the original, so the
    /// reader can always see both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) original_text: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) important: bool,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

impl TranscriptSegmentView {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_nil()
            || self.end_ms <= self.start_ms
            || self.end_ms > JSON_SAFE_INTEGER_MAX
            || self.text.is_empty()
            || self.text.len() > MAX_TRANSCRIPT_TEXT_BYTES
            || self.text.chars().any(|value| {
                value == '\r' || (value.is_control() && value != '\n' && value != '\t')
            })
            || !valid_language(&self.language)
        {
            return Err("transcript_segment_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptPageRequest {
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cursor: Option<String>,
    pub(crate) limit: u16,
}

impl TranscriptPageRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !(1..=MAX_TRANSCRIPT_PAGE_LIMIT).contains(&self.limit)
            || self
                .cursor
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > MAX_TRANSCRIPT_CURSOR_BYTES)
        {
            return Err("transcript_page_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptPage {
    pub(crate) items: Vec<TranscriptSegmentView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<String>,
}

impl TranscriptPage {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.items.len() > usize::from(MAX_TRANSCRIPT_PAGE_LIMIT)
            || self
                .next_cursor
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > MAX_TRANSCRIPT_CURSOR_BYTES)
        {
            return Err("transcript_page_invalid");
        }
        self.items
            .iter()
            .try_for_each(TranscriptSegmentView::validate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptSearchQuery {
    pub(crate) project_id: ProjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<SessionId>,
    pub(crate) query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cursor: Option<String>,
    pub(crate) limit: u16,
}

impl TranscriptSearchQuery {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let terms = self.query.split_whitespace().count();
        if self.query.is_empty()
            || self.query.len() > MAX_TRANSCRIPT_QUERY_BYTES
            || self.query.chars().any(char::is_control)
            || terms == 0
            || terms > MAX_TRANSCRIPT_QUERY_TERMS
            || !(1..=MAX_TRANSCRIPT_PAGE_LIMIT).contains(&self.limit)
            || self
                .cursor
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > MAX_TRANSCRIPT_CURSOR_BYTES)
        {
            return Err("transcript_search_request_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptSearchHit {
    pub(crate) id: Uuid,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) language: String,
    pub(crate) snippet: String,
}

impl TranscriptSearchHit {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_nil()
            || self.end_ms <= self.start_ms
            || self.end_ms > JSON_SAFE_INTEGER_MAX
            || !valid_language(&self.language)
            || self.snippet.is_empty()
            || self.snippet.chars().count() > MAX_TRANSCRIPT_SNIPPET_CHARS
            || self.snippet.chars().any(char::is_control)
        {
            return Err("transcript_search_result_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptSearchPageView {
    pub(crate) items: Vec<TranscriptSearchHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<String>,
}

impl TranscriptSearchPageView {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.items.len() > usize::from(MAX_TRANSCRIPT_PAGE_LIMIT)
            || self
                .next_cursor
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > MAX_TRANSCRIPT_CURSOR_BYTES)
        {
            return Err("transcript_search_result_invalid");
        }
        self.items
            .iter()
            .try_for_each(TranscriptSearchHit::validate)
    }
}

fn valid_language(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TRANSCRIPT_LANGUAGE_BYTES
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::{
        TranscriptPage, TranscriptPageRequest, TranscriptSearchPageView, TranscriptSearchQuery,
    };

    #[test]
    fn shared_transcript_contract_fixture_is_strict_and_valid() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/transcript-reading-v1.json"
        ))
        .expect("fixture JSON");
        let page_request: TranscriptPageRequest =
            serde_json::from_value(fixture["pageRequest"].clone()).unwrap();
        let page: TranscriptPage = serde_json::from_value(fixture["page"].clone()).unwrap();
        let search_request: TranscriptSearchQuery =
            serde_json::from_value(fixture["searchRequest"].clone()).unwrap();
        let search_page: TranscriptSearchPageView =
            serde_json::from_value(fixture["searchPage"].clone()).unwrap();
        assert!(page_request.validate().is_ok());
        assert!(page.validate().is_ok());
        assert!(search_request.validate().is_ok());
        assert!(search_page.validate().is_ok());

        let mut invalid = fixture["searchRequest"].clone();
        invalid["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TranscriptSearchQuery>(invalid).is_err());
    }
}

/// What the user is saying about one finalized segment.
///
/// Correcting and marking are separate because they mean different things: one
/// changes what the transcript reads as, the other only flags it. Both are
/// recorded as events, so neither erases what was transcribed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum SegmentAnnotation {
    #[serde(rename_all = "camelCase")]
    Correction { text: String },
    #[serde(rename_all = "camelCase")]
    Importance { important: bool },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum RawSegmentAnnotation {
    #[serde(rename_all = "camelCase")]
    Correction { text: String },
    #[serde(rename_all = "camelCase")]
    Importance { important: bool },
}

impl<'de> Deserialize<'de> for SegmentAnnotation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let annotation = match RawSegmentAnnotation::deserialize(deserializer)? {
            RawSegmentAnnotation::Correction { text } => Self::Correction { text },
            RawSegmentAnnotation::Importance { important } => Self::Importance { important },
        };
        annotation.validate().map_err(serde::de::Error::custom)?;
        Ok(annotation)
    }
}

impl SegmentAnnotation {
    /// A correction is rendered exactly where the transcription was, so it
    /// obeys the same bounds a transcription does.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let Self::Correction { text } = self else {
            return Ok(());
        };
        if text.trim().is_empty()
            || text.len() > MAX_TRANSCRIPT_TEXT_BYTES
            || text.chars().any(|value| {
                value == '\r' || (value.is_control() && value != '\n' && value != '\t')
            })
        {
            return Err("transcript_annotation_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AnnotateTranscriptSegmentRequest {
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) segment_id: Uuid,
    pub(crate) annotation: SegmentAnnotation,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAnnotateTranscriptSegmentRequest {
    project_id: ProjectId,
    session_id: SessionId,
    segment_id: Uuid,
    annotation: SegmentAnnotation,
}

impl<'de> Deserialize<'de> for AnnotateTranscriptSegmentRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawAnnotateTranscriptSegmentRequest::deserialize(deserializer)?;
        let request = Self {
            project_id: raw.project_id,
            session_id: raw.session_id,
            segment_id: raw.segment_id,
            annotation: raw.annotation,
        };
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

impl AnnotateTranscriptSegmentRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.segment_id.is_nil()
        {
            return Err("transcript_annotation_invalid");
        }
        self.annotation.validate()
    }
}
