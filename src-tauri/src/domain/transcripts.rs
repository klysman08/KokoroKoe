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
