use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::audio::DeviceSelection;

use super::settings::PresetId;

const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub(crate) struct ProjectId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub(crate) struct SessionId(Uuid);

impl ProjectId {
    fn suffix(self) -> String {
        self.0.simple().to_string()[..8].to_owned()
    }
}

impl SessionId {
    fn suffix(self) -> String {
        self.0.simple().to_string()[..8].to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LlmRoleModels {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) insights: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summaries: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manual_questions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateProjectInput {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) global_context: String,
    pub(crate) participants: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) default_preset_id: PresetId,
    pub(crate) default_transcription_model_id: String,
    pub(crate) preferred_llm_models: LlmRoleModels,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateProjectInput {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) global_context: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) participants: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_preset_id: Option<PresetId>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_transcription_model_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) preferred_llm_models: Option<LlmRoleModels>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PageRequest {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cursor: Option<String>,
    pub(crate) limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectPage {
    pub(crate) items: Vec<Project>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawProjectPage {
    items: Vec<Project>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    next_cursor: Option<String>,
}

impl<'de> Deserialize<'de> for ProjectPage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawProjectPage::deserialize(deserializer)?;
        if raw.items.len() > 100 || !valid_cursor(raw.next_cursor.as_deref()) {
            return Err(D::Error::custom("project_page_invalid"));
        }
        Ok(Self {
            items: raw.items,
            next_cursor: raw.next_cursor,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InsightType {
    SuggestedResponse,
    FollowUpQuestion,
    Clarification,
    FactOrNumber,
    Risk,
    Objection,
    Decision,
    ActionItem,
    Contradiction,
    UnaddressedTopic,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PresetSnapshot {
    pub(crate) id: PresetId,
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) assistant_role: String,
    pub(crate) analysis_objectives: Vec<String>,
    pub(crate) insight_types: Vec<InsightType>,
    pub(crate) response_tone: String,
    pub(crate) final_summary_sections: Vec<String>,
    pub(crate) highlight_instructions: Vec<String>,
    pub(crate) prohibited_behaviors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioDeviceSnapshot {
    pub(crate) endpoint_id: String,
    pub(crate) friendly_name: String,
    pub(crate) selection: DeviceSelection,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) native_sample_rate: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) native_channels: Option<u16>,
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
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail_code: Option<String>,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ChannelHealthBySource {
    pub(crate) microphone: ChannelHealth,
    pub(crate) system_output: ChannelHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageAggregate {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) estimated_cost_usd: String,
    pub(crate) actual_cost_usd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionState {
    Idle,
    Preparing,
    Capturing,
    Transcribing,
    Paused,
    Stopping,
    ProcessingSummary,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SummaryStatus {
    NotRequested,
    Pending,
    Completed,
    Deferred,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateSessionSnapshotInput {
    pub(crate) title: String,
    pub(crate) objective: String,
    pub(crate) session_context: String,
    pub(crate) preset: PresetSnapshot,
    pub(crate) language: String,
    pub(crate) microphone: AudioDeviceSnapshot,
    pub(crate) system_output: AudioDeviceSnapshot,
    pub(crate) transcription_model_id: String,
    pub(crate) llm_models: LlmRoleModels,
    pub(crate) retain_audio: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateSessionInput {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) objective: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_context: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) preset: Option<PresetSnapshot>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) microphone: Option<AudioDeviceSnapshot>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system_output: Option<AudioDeviceSnapshot>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transcription_model_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) llm_models: Option<LlmRoleModels>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) retain_audio: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Project {
    pub(crate) schema_version: u8,
    pub(crate) id: ProjectId,
    pub(crate) name: String,
    pub(crate) folder_name: String,
    pub(crate) description: String,
    pub(crate) global_context: String,
    pub(crate) participants: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) default_preset_id: PresetId,
    pub(crate) default_transcription_model_id: String,
    pub(crate) preferred_llm_models: LlmRoleModels,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawProject {
    schema_version: u8,
    id: ProjectId,
    name: String,
    folder_name: String,
    description: String,
    global_context: String,
    participants: Vec<String>,
    tags: Vec<String>,
    default_preset_id: PresetId,
    default_transcription_model_id: String,
    preferred_llm_models: LlmRoleModels,
    created_at: String,
    updated_at: String,
    revision: u64,
}

impl<'de> Deserialize<'de> for Project {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawProject::deserialize(deserializer)?;
        let value = Self {
            schema_version: raw.schema_version,
            id: raw.id,
            name: raw.name,
            folder_name: raw.folder_name,
            description: raw.description,
            global_context: raw.global_context,
            participants: raw.participants,
            tags: raw.tags,
            default_preset_id: raw.default_preset_id,
            default_transcription_model_id: raw.default_transcription_model_id,
            preferred_llm_models: raw.preferred_llm_models,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
            revision: raw.revision,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Session {
    pub(crate) schema_version: u8,
    pub(crate) id: SessionId,
    pub(crate) project_id: ProjectId,
    pub(crate) folder_name: String,
    pub(crate) title: String,
    pub(crate) objective: String,
    pub(crate) session_context: String,
    pub(crate) preset: PresetSnapshot,
    pub(crate) language: String,
    pub(crate) microphone: AudioDeviceSnapshot,
    pub(crate) system_output: AudioDeviceSnapshot,
    pub(crate) transcription_engine: TranscriptionEngine,
    pub(crate) transcription_model_id: String,
    pub(crate) llm_models: LlmRoleModels,
    pub(crate) retain_audio: bool,
    pub(crate) state: SessionState,
    pub(crate) channel_health: ChannelHealthBySource,
    pub(crate) summary_status: SummaryStatus,
    pub(crate) usage: UsageAggregate,
    pub(crate) created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ended_at: Option<String>,
    pub(crate) updated_at: String,
    pub(crate) revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionPage {
    pub(crate) items: Vec<Session>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSessionPage {
    items: Vec<Session>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    next_cursor: Option<String>,
}

impl<'de> Deserialize<'de> for SessionPage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSessionPage::deserialize(deserializer)?;
        if raw.items.len() > 100 || !valid_session_cursor(raw.next_cursor.as_deref()) {
            return Err(D::Error::custom("session_page_invalid"));
        }
        Ok(Self {
            items: raw.items,
            next_cursor: raw.next_cursor,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TranscriptionEngine {
    Whisper,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSession {
    schema_version: u8,
    id: SessionId,
    project_id: ProjectId,
    folder_name: String,
    title: String,
    objective: String,
    session_context: String,
    preset: PresetSnapshot,
    language: String,
    microphone: AudioDeviceSnapshot,
    system_output: AudioDeviceSnapshot,
    transcription_engine: TranscriptionEngine,
    transcription_model_id: String,
    llm_models: LlmRoleModels,
    retain_audio: bool,
    state: SessionState,
    channel_health: ChannelHealthBySource,
    summary_status: SummaryStatus,
    usage: UsageAggregate,
    created_at: String,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    started_at: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    ended_at: Option<String>,
    updated_at: String,
    revision: u64,
}

impl<'de> Deserialize<'de> for Session {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSession::deserialize(deserializer)?;
        let value = Self {
            schema_version: raw.schema_version,
            id: raw.id,
            project_id: raw.project_id,
            folder_name: raw.folder_name,
            title: raw.title,
            objective: raw.objective,
            session_context: raw.session_context,
            preset: raw.preset,
            language: raw.language,
            microphone: raw.microphone,
            system_output: raw.system_output,
            transcription_engine: raw.transcription_engine,
            transcription_model_id: raw.transcription_model_id,
            llm_models: raw.llm_models,
            retain_audio: raw.retain_audio,
            state: raw.state,
            channel_health: raw.channel_health,
            summary_status: raw.summary_status,
            usage: raw.usage,
            created_at: raw.created_at,
            started_at: raw.started_at,
            ended_at: raw.ended_at,
            updated_at: raw.updated_at,
            revision: raw.revision,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

impl Project {
    #[allow(dead_code)]
    pub(crate) fn derive_folder_name(name: &str, id: ProjectId) -> String {
        format!("{}--{}", portable_slug(name, "project"), id.suffix())
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || !bounded_text(&self.name, 1, 128, false)
            || !bounded_text(&self.description, 0, 4_096, true)
            || !bounded_text(&self.global_context, 0, 32_768, true)
            || !bounded_text(&self.default_transcription_model_id, 1, 128, false)
            || !valid_string_list(&self.participants, 64, 128)
            || !valid_string_list(&self.tags, 64, 64)
            || !valid_llm_models(&self.preferred_llm_models)
            || !valid_folder_name(&self.folder_name, None, &self.id.suffix())
            || self.revision > JSON_SAFE_INTEGER_MAX
            || !ordered_timestamps(&self.created_at, &self.updated_at)
        {
            return Err("project_contract_invalid");
        }
        Ok(())
    }

    pub(crate) fn create(
        input: CreateProjectInput,
        created_at: String,
    ) -> Result<Self, &'static str> {
        parse_timestamp(&created_at).ok_or("project_contract_invalid")?;
        let id = ProjectId(Uuid::new_v4());
        let value = Self {
            schema_version: 1,
            id,
            folder_name: Self::derive_folder_name(&input.name, id),
            name: input.name,
            description: input.description,
            global_context: input.global_context,
            participants: input.participants,
            tags: input.tags,
            default_preset_id: input.default_preset_id,
            default_transcription_model_id: input.default_transcription_model_id,
            preferred_llm_models: input.preferred_llm_models,
            created_at: created_at.clone(),
            updated_at: created_at,
            revision: 1,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn apply_update(
        &self,
        update: UpdateProjectInput,
        updated_at: String,
    ) -> Result<Self, &'static str> {
        if update.is_empty() {
            return Err("project_update_invalid");
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= JSON_SAFE_INTEGER_MAX)
            .ok_or("project_revision_exhausted")?;
        let current_time = parse_timestamp(&self.updated_at).ok_or("project_contract_invalid")?;
        let requested_time = parse_timestamp(&updated_at).ok_or("project_contract_invalid")?;
        let updated_at = if requested_time >= current_time {
            updated_at
        } else {
            self.updated_at.clone()
        };
        let value = Self {
            schema_version: self.schema_version,
            id: self.id,
            name: update.name.unwrap_or_else(|| self.name.clone()),
            folder_name: self.folder_name.clone(),
            description: update
                .description
                .unwrap_or_else(|| self.description.clone()),
            global_context: update
                .global_context
                .unwrap_or_else(|| self.global_context.clone()),
            participants: update
                .participants
                .unwrap_or_else(|| self.participants.clone()),
            tags: update.tags.unwrap_or_else(|| self.tags.clone()),
            default_preset_id: update
                .default_preset_id
                .unwrap_or_else(|| self.default_preset_id.clone()),
            default_transcription_model_id: update
                .default_transcription_model_id
                .unwrap_or_else(|| self.default_transcription_model_id.clone()),
            preferred_llm_models: update
                .preferred_llm_models
                .unwrap_or_else(|| self.preferred_llm_models.clone()),
            created_at: self.created_at.clone(),
            updated_at,
            revision: next_revision,
        };
        value.validate()?;
        Ok(value)
    }
}

impl UpdateProjectInput {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.global_context.is_none()
            && self.participants.is_none()
            && self.tags.is_none()
            && self.default_preset_id.is_none()
            && self.default_transcription_model_id.is_none()
            && self.preferred_llm_models.is_none()
    }
}

impl PageRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.limit == 0 || self.limit > 100 || !valid_cursor(self.cursor.as_deref()) {
            return Err("project_page_invalid");
        }
        Ok(())
    }
}

fn valid_cursor(cursor: Option<&str>) -> bool {
    cursor.is_none_or(|cursor| {
        !cursor.is_empty()
            && cursor.len() <= 128
            && cursor.chars().all(|character| !character.is_control())
    })
}

impl Session {
    #[allow(dead_code)]
    pub(crate) fn derive_folder_name(
        title: &str,
        id: SessionId,
        created_at: &str,
    ) -> Option<String> {
        parse_timestamp(created_at)?;
        let date = created_at.get(..10)?;
        Some(format!(
            "{date}-{}--{}",
            portable_slug(title, "session"),
            id.suffix()
        ))
    }

    #[allow(dead_code)]
    pub(crate) fn create(
        project_id: ProjectId,
        input: CreateSessionSnapshotInput,
        created_at: String,
    ) -> Result<Self, &'static str> {
        parse_timestamp(&created_at).ok_or("session_contract_invalid")?;
        let id = SessionId(Uuid::new_v4());
        let folder_name = Self::derive_folder_name(&input.title, id, &created_at)
            .ok_or("session_contract_invalid")?;
        let channel_health = ChannelHealthBySource {
            microphone: ChannelHealth {
                status: ChannelHealthStatus::Stopped,
                endpoint_id: Some(input.microphone.endpoint_id.clone()),
                detail_code: None,
                updated_at: created_at.clone(),
            },
            system_output: ChannelHealth {
                status: ChannelHealthStatus::Stopped,
                endpoint_id: Some(input.system_output.endpoint_id.clone()),
                detail_code: None,
                updated_at: created_at.clone(),
            },
        };
        let value = Self {
            schema_version: 1,
            id,
            project_id,
            folder_name,
            title: input.title,
            objective: input.objective,
            session_context: input.session_context,
            preset: input.preset,
            language: input.language,
            microphone: input.microphone,
            system_output: input.system_output,
            transcription_engine: TranscriptionEngine::Whisper,
            transcription_model_id: input.transcription_model_id,
            llm_models: input.llm_models,
            retain_audio: input.retain_audio,
            state: SessionState::Idle,
            channel_health,
            summary_status: SummaryStatus::NotRequested,
            usage: UsageAggregate {
                input_tokens: 0,
                output_tokens: 0,
                estimated_cost_usd: "0.00".to_owned(),
                actual_cost_usd: "0.00".to_owned(),
            },
            created_at: created_at.clone(),
            started_at: None,
            ended_at: None,
            updated_at: created_at,
            revision: 1,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let created_date = self.created_at.get(..10);
        if self.schema_version != 1
            || !bounded_text(&self.title, 1, 256, false)
            || !bounded_text(&self.objective, 0, 4_096, true)
            || !bounded_text(&self.session_context, 0, 32_768, true)
            || !valid_language(&self.language)
            || !valid_device_snapshot(&self.microphone)
            || !valid_device_snapshot(&self.system_output)
            || !valid_preset(&self.preset)
            || !bounded_text(&self.transcription_model_id, 1, 128, false)
            || !valid_llm_models(&self.llm_models)
            || !valid_channel_health(&self.channel_health.microphone)
            || !valid_channel_health(&self.channel_health.system_output)
            || !valid_usage(&self.usage)
            || !valid_folder_name(&self.folder_name, created_date, &self.id.suffix())
            || self.revision > JSON_SAFE_INTEGER_MAX
            || !ordered_timestamps(&self.created_at, &self.updated_at)
            || !valid_lifecycle_timestamps(self)
        {
            return Err("session_contract_invalid");
        }
        Ok(())
    }

    pub(crate) fn apply_update(
        &self,
        update: UpdateSessionInput,
        updated_at: String,
    ) -> Result<Self, &'static str> {
        if update.is_empty() || self.state != SessionState::Idle {
            return Err("session_update_invalid");
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= JSON_SAFE_INTEGER_MAX)
            .ok_or("session_revision_exhausted")?;
        let current_time = parse_timestamp(&self.updated_at).ok_or("session_contract_invalid")?;
        let requested_time = parse_timestamp(&updated_at).ok_or("session_contract_invalid")?;
        let updated_at = if requested_time >= current_time {
            updated_at
        } else {
            self.updated_at.clone()
        };
        let microphone = update.microphone.unwrap_or_else(|| self.microphone.clone());
        let system_output = update
            .system_output
            .unwrap_or_else(|| self.system_output.clone());
        let mut value = self.clone();
        value.title = update.title.unwrap_or_else(|| self.title.clone());
        value.objective = update.objective.unwrap_or_else(|| self.objective.clone());
        value.session_context = update
            .session_context
            .unwrap_or_else(|| self.session_context.clone());
        value.preset = update.preset.unwrap_or_else(|| self.preset.clone());
        value.language = update.language.unwrap_or_else(|| self.language.clone());
        value.microphone = microphone.clone();
        value.system_output = system_output.clone();
        value.transcription_model_id = update
            .transcription_model_id
            .unwrap_or_else(|| self.transcription_model_id.clone());
        value.llm_models = update.llm_models.unwrap_or_else(|| self.llm_models.clone());
        value.retain_audio = update.retain_audio.unwrap_or(self.retain_audio);
        value.channel_health.microphone.endpoint_id = Some(microphone.endpoint_id);
        value.channel_health.system_output.endpoint_id = Some(system_output.endpoint_id);
        value.updated_at = updated_at;
        value.revision = next_revision;
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn apply_runtime_state(
        &self,
        state: SessionState,
        updated_at: String,
        detail_code: Option<&str>,
    ) -> Result<Self, &'static str> {
        let allowed = matches!(
            (self.state, state),
            (
                SessionState::Idle,
                SessionState::Transcribing
                    | SessionState::Paused
                    | SessionState::Completed
                    | SessionState::Failed
            ) | (
                SessionState::Paused,
                SessionState::Transcribing | SessionState::Completed | SessionState::Failed
            ) | (
                SessionState::Transcribing,
                SessionState::Paused | SessionState::Completed | SessionState::Failed
            ) | (
                SessionState::Preparing | SessionState::Capturing | SessionState::Stopping,
                SessionState::Failed
            )
        );
        if !allowed || detail_code.is_some_and(|code| !bounded_text(code, 1, 128, false)) {
            return Err("session_lifecycle_invalid");
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= JSON_SAFE_INTEGER_MAX)
            .ok_or("session_revision_exhausted")?;
        let current_time = parse_timestamp(&self.updated_at).ok_or("session_contract_invalid")?;
        let requested_time = parse_timestamp(&updated_at).ok_or("session_contract_invalid")?;
        let updated_at = if requested_time >= current_time {
            updated_at
        } else {
            self.updated_at.clone()
        };
        let health = match state {
            SessionState::Transcribing => ChannelHealthStatus::Active,
            SessionState::Failed => ChannelHealthStatus::Unavailable,
            SessionState::Paused | SessionState::Completed => ChannelHealthStatus::Stopped,
            _ => return Err("session_lifecycle_invalid"),
        };
        let mut value = self.clone();
        value.state = state;
        value.started_at = value.started_at.or_else(|| {
            matches!(
                state,
                SessionState::Transcribing | SessionState::Paused | SessionState::Completed
            )
            .then(|| updated_at.clone())
        });
        if state == SessionState::Completed {
            value.ended_at = Some(updated_at.clone());
        }
        for channel in [
            &mut value.channel_health.microphone,
            &mut value.channel_health.system_output,
        ] {
            channel.status = health;
            channel.detail_code = detail_code.map(str::to_owned);
            channel.updated_at = updated_at.clone();
        }
        value.updated_at = updated_at;
        value.revision = next_revision;
        value.validate()?;
        Ok(value)
    }
}

impl UpdateSessionInput {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.objective.is_none()
            && self.session_context.is_none()
            && self.preset.is_none()
            && self.language.is_none()
            && self.microphone.is_none()
            && self.system_output.is_none()
            && self.transcription_model_id.is_none()
            && self.llm_models.is_none()
            && self.retain_audio.is_none()
    }
}

fn valid_session_cursor(cursor: Option<&str>) -> bool {
    cursor.is_none_or(|cursor| {
        let Some(encoded) = cursor.strip_prefix("s1:") else {
            return false;
        };
        let mut parts = encoded.split(':');
        let generation = parts.next();
        let project_id = parts.next();
        let offset = parts.next();
        parts.next().is_none()
            && generation.is_some_and(|value| value.parse::<u64>().is_ok_and(|value| value >= 1))
            && project_id.is_some_and(|value| Uuid::parse_str(value).is_ok())
            && offset.is_some_and(|value| value.parse::<u32>().is_ok())
    })
}

fn valid_lifecycle_timestamps(session: &Session) -> bool {
    let created = parse_timestamp(&session.created_at);
    let started = session.started_at.as_deref().and_then(parse_timestamp);
    let ended = session.ended_at.as_deref().and_then(parse_timestamp);
    if session.started_at.is_some() != started.is_some()
        || session.ended_at.is_some() != ended.is_some()
    {
        return false;
    }
    if started.is_some_and(|value| created.is_none_or(|created| value < created))
        || ended.is_some_and(|value| started.or(created).is_none_or(|start| value < start))
    {
        return false;
    }
    session.state != SessionState::Completed || session.ended_at.is_some()
}

fn valid_device_snapshot(value: &AudioDeviceSnapshot) -> bool {
    bounded_text(&value.endpoint_id, 1, 1_024, false)
        && bounded_text(&value.friendly_name, 1, 512, false)
        && value.native_sample_rate != Some(0)
        && value.native_channels != Some(0)
        && match &value.selection {
            DeviceSelection::Default { .. } => true,
            DeviceSelection::Fixed { endpoint_id } => endpoint_id == &value.endpoint_id,
        }
}

fn valid_preset(value: &PresetSnapshot) -> bool {
    value.version > 0
        && bounded_text(&value.name, 1, 128, false)
        && bounded_text(&value.assistant_role, 1, 4_096, true)
        && bounded_text(&value.response_tone, 1, 1_024, true)
        && valid_string_list(&value.analysis_objectives, 32, 1_024)
        && !value.analysis_objectives.is_empty()
        && !value.insight_types.is_empty()
        && value.insight_types.len() <= 16
        && value
            .insight_types
            .iter()
            .enumerate()
            .all(|(index, insight)| {
                value.insight_types[..index]
                    .iter()
                    .all(|previous| previous != insight)
            })
        && valid_string_list(&value.final_summary_sections, 32, 128)
        && !value.final_summary_sections.is_empty()
        && valid_string_list(&value.highlight_instructions, 32, 1_024)
        && valid_string_list(&value.prohibited_behaviors, 32, 1_024)
}

fn valid_channel_health(value: &ChannelHealth) -> bool {
    value
        .endpoint_id
        .as_deref()
        .is_none_or(|text| bounded_text(text, 1, 1_024, false))
        && value
            .detail_code
            .as_deref()
            .is_none_or(|text| bounded_text(text, 1, 128, false))
        && parse_timestamp(&value.updated_at).is_some()
}

fn valid_usage(value: &UsageAggregate) -> bool {
    value.input_tokens <= JSON_SAFE_INTEGER_MAX
        && value.output_tokens <= JSON_SAFE_INTEGER_MAX
        && fixed_decimal(&value.estimated_cost_usd)
        && fixed_decimal(&value.actual_cost_usd)
}

fn valid_llm_models(value: &LlmRoleModels) -> bool {
    [&value.insights, &value.summaries, &value.manual_questions]
        .into_iter()
        .all(|model| {
            model
                .as_deref()
                .is_none_or(|text| bounded_text(text, 1, 256, false))
        })
}

fn valid_language(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    (1..=8).contains(&parts.len())
        && value.len() <= 64
        && parts.iter().all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        && parts[0].len() >= 2
        && parts[0].bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn valid_folder_name(value: &str, date: Option<&str>, suffix: &str) -> bool {
    if value.len() > 80 || value.contains(['/', '\\']) || !value.ends_with(&format!("--{suffix}")) {
        return false;
    }
    let stem = &value[..value.len() - suffix.len() - 2];
    let slug = if let Some(date) = date {
        if !stem.starts_with(date) || stem.as_bytes().get(10) != Some(&b'-') {
            return false;
        }
        &stem[11..]
    } else {
        stem
    };
    !slug.is_empty()
        && slug.len() <= 48
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn portable_slug(value: &str, fallback: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() && slug.len() < 48 {
                slug.push('-');
            }
            pending_separator = false;
            if slug.len() < 48 {
                slug.push(byte.to_ascii_lowercase() as char);
            }
        } else {
            pending_separator = !slug.is_empty();
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        fallback.to_owned()
    } else {
        slug
    }
}

fn valid_string_list(values: &[String], maximum_items: usize, maximum_text: usize) -> bool {
    values.len() <= maximum_items
        && values
            .iter()
            .all(|value| bounded_text(value, 1, maximum_text, true))
        && values
            .iter()
            .enumerate()
            .all(|(index, value)| values[..index].iter().all(|previous| previous != value))
}

fn bounded_text(value: &str, minimum: usize, maximum: usize, multiline: bool) -> bool {
    let length = value.encode_utf16().count();
    (minimum..=maximum).contains(&length)
        && value.chars().all(|character| {
            (!character.is_control() || (multiline && matches!(character, '\n' | '\r' | '\t')))
                && character != '\0'
        })
}

fn fixed_decimal(value: &str) -> bool {
    let Some((whole, fraction)) = value.split_once('.') else {
        return false;
    };
    value.len() <= 18
        && !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.len() == 2
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn ordered_timestamps(created: &str, updated: &str) -> bool {
    parse_timestamp(created)
        .zip(parse_timestamp(updated))
        .is_some_and(|(created, updated)| updated >= created)
}

fn deserialize_non_null_optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::Value;

    use super::{
        CreateProjectInput, CreateSessionSnapshotInput, PageRequest, Project, ProjectId,
        ProjectPage, Session, SessionId, SessionPage, SessionState, SummaryStatus,
        UpdateProjectInput, UpdateSessionInput,
    };
    use crate::persistence::layout::PortableFolderContract;

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .expect("project/session fixture should be valid JSON")
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ProjectManagementFixture {
        page_request: PageRequest,
        page: ProjectPage,
        create_input: CreateProjectInput,
        update_request: ProjectUpdateRequest,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ProjectUpdateRequest {
        project_id: ProjectId,
        expected_revision: u64,
        value: UpdateProjectInput,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SessionManagementFixture {
        page_request: SessionPageRequest,
        page: SessionPage,
        create_request: SessionCreateRequest,
        update_request: SessionUpdateRequest,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SessionPageRequest {
        project_id: ProjectId,
        #[serde(flatten)]
        page: PageRequest,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SessionCreateRequest {
        project_id: ProjectId,
        value: CreateSessionSnapshotInput,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SessionUpdateRequest {
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
        value: UpdateSessionInput,
    }

    fn management_fixture() -> ProjectManagementFixture {
        serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-management-v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn session_management_matches_the_shared_contract() {
        let fixture: SessionManagementFixture = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/session-management-v1.json"
        ))
        .unwrap();
        assert_eq!(fixture.page_request.page.limit, 12);
        assert!(fixture.page.items.is_empty());
        assert_eq!(
            fixture.create_request.project_id,
            fixture.page_request.project_id
        );
        assert_eq!(fixture.create_request.value.title, "Sprint planning");
        assert_eq!(
            fixture.update_request.project_id,
            fixture.page_request.project_id
        );
        assert_eq!(fixture.update_request.expected_revision, 1);
        assert_eq!(
            serde_json::to_value(fixture.update_request.session_id).unwrap(),
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
        );
        assert_eq!(
            fixture.update_request.value.title.as_deref(),
            Some("Sprint planning review")
        );
    }

    #[test]
    fn project_session_and_layout_match_the_shared_contract() {
        let fixture = fixture();
        let project: Project = serde_json::from_value(fixture["project"].clone()).unwrap();
        let session: Session = serde_json::from_value(fixture["session"].clone()).unwrap();
        let layout = PortableFolderContract::for_records(&project, &session).unwrap();

        assert_eq!(serde_json::to_value(&project).unwrap(), fixture["project"]);
        assert_eq!(serde_json::to_value(&session).unwrap(), fixture["session"]);
        assert_eq!(serde_json::to_value(layout).unwrap(), fixture["layout"]);
        assert_eq!(
            Project::derive_folder_name(&project.name, project.id),
            project.folder_name
        );
        assert_eq!(
            Session::derive_folder_name(&session.title, session.id, &session.created_at).as_deref(),
            Some(session.folder_name.as_str())
        );
        assert_eq!(
            Project::derive_folder_name("会議", project.id),
            "project--aaaaaaaa"
        );
    }

    #[test]
    fn session_creation_derives_idle_revision_one_state_from_resolved_snapshots() {
        let fixture = fixture();
        let project: Project = serde_json::from_value(fixture["project"].clone()).unwrap();
        let source = &fixture["session"];
        let input: CreateSessionSnapshotInput = serde_json::from_value(serde_json::json!({
            "title": source["title"],
            "objective": source["objective"],
            "sessionContext": source["sessionContext"],
            "preset": source["preset"],
            "language": source["language"],
            "microphone": source["microphone"],
            "systemOutput": source["systemOutput"],
            "transcriptionModelId": source["transcriptionModelId"],
            "llmModels": source["llmModels"],
            "retainAudio": true
        }))
        .unwrap();

        let session =
            Session::create(project.id, input, "2026-08-11T08:45:00Z".to_owned()).unwrap();

        assert_eq!(session.state, SessionState::Idle);
        assert_eq!(session.summary_status, SummaryStatus::NotRequested);
        assert_eq!(session.revision, 1);
        assert!(session.started_at.is_none());
        assert!(session.ended_at.is_none());
        assert!(session.retain_audio);
        assert!(
            session
                .folder_name
                .starts_with("2026-08-11-sprint-planning--")
        );
        assert_eq!(session.usage.estimated_cost_usd, "0.00");
        assert!(session.validate().is_ok());
    }

    #[test]
    fn session_creation_rejects_invalid_or_unknown_input() {
        let fixture = fixture();
        let project: Project = serde_json::from_value(fixture["project"].clone()).unwrap();
        let source = &fixture["session"];
        let mut input = serde_json::json!({
            "title": source["title"],
            "objective": source["objective"],
            "sessionContext": source["sessionContext"],
            "preset": source["preset"],
            "language": source["language"],
            "microphone": source["microphone"],
            "systemOutput": source["systemOutput"],
            "transcriptionModelId": source["transcriptionModelId"],
            "llmModels": source["llmModels"],
            "retainAudio": false
        });
        input["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CreateSessionSnapshotInput>(input).is_err());

        let mut input: CreateSessionSnapshotInput = serde_json::from_value(serde_json::json!({
            "title": source["title"],
            "objective": source["objective"],
            "sessionContext": source["sessionContext"],
            "preset": source["preset"],
            "language": source["language"],
            "microphone": source["microphone"],
            "systemOutput": source["systemOutput"],
            "transcriptionModelId": source["transcriptionModelId"],
            "llmModels": source["llmModels"],
            "retainAudio": false
        }))
        .unwrap();
        input.title.clear();
        assert_eq!(
            Session::create(project.id, input, "2026-08-11T08:45:00Z".to_owned()),
            Err("session_contract_invalid")
        );
    }

    #[test]
    fn project_rejects_unknown_invalid_or_inconsistent_values() {
        let base = fixture()["project"].clone();
        for (field, value) in [
            ("schemaVersion", serde_json::json!(2)),
            ("id", serde_json::json!("not-a-uuid")),
            ("name", serde_json::json!("")),
            ("folderName", serde_json::json!("escape/meeting")),
            ("revision", serde_json::json!(9_007_199_254_740_992_u64)),
            ("createdAt", serde_json::json!("not-a-time")),
        ] {
            let mut invalid = base.clone();
            invalid[field] = value;
            assert!(
                serde_json::from_value::<Project>(invalid).is_err(),
                "{field}"
            );
        }
        let mut unknown = base;
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Project>(unknown).is_err());

        let mut duplicate = fixture()["project"].clone();
        duplicate["tags"] = serde_json::json!(["weekly", "weekly"]);
        assert!(serde_json::from_value::<Project>(duplicate).is_err());
    }

    #[test]
    fn session_rejects_null_unknown_and_cross_field_inconsistency() {
        let base = fixture()["session"].clone();
        for (field, value) in [
            ("startedAt", Value::Null),
            ("language", serde_json::json!("bad_language")),
            ("folderName", serde_json::json!("2026-08-10-demo--ffffffff")),
            ("state", serde_json::json!("completed")),
        ] {
            let mut invalid = base.clone();
            invalid[field] = value;
            assert!(
                serde_json::from_value::<Session>(invalid).is_err(),
                "{field}"
            );
        }
        let mut fixed_mismatch = base.clone();
        fixed_mismatch["microphone"]["selection"] =
            serde_json::json!({"kind":"fixed","endpointId":"different"});
        assert!(serde_json::from_value::<Session>(fixed_mismatch).is_err());

        let mut duplicate = base.clone();
        duplicate["preset"]["insightTypes"] = serde_json::json!(["decision", "decision"]);
        assert!(serde_json::from_value::<Session>(duplicate).is_err());

        let mut unknown = base;
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Session>(unknown).is_err());
    }

    #[test]
    fn project_management_requests_and_page_match_the_shared_contract() {
        let fixture = management_fixture();
        fixture.page_request.validate().unwrap();
        assert_eq!(fixture.page.items.len(), 1);
        assert_eq!(fixture.update_request.expected_revision, 2);
        assert_eq!(fixture.update_request.project_id, fixture.page.items[0].id);

        let created =
            Project::create(fixture.create_input, "2026-08-11T12:00:00Z".to_owned()).unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            created.folder_name,
            "customer-discovery--".to_owned() + &created.id.suffix()
        );
        let updated = fixture.page.items[0]
            .apply_update(
                fixture.update_request.value,
                "2026-08-11T13:00:00Z".to_owned(),
            )
            .unwrap();
        assert_eq!(updated.revision, 3);
        assert_eq!(updated.folder_name, fixture.page.items[0].folder_name);
        assert_eq!(updated.name, "Weekly product review");
    }

    #[test]
    fn project_management_rejects_empty_nullable_unknown_and_unbounded_requests() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-management-v1.json"
        ))
        .unwrap();
        let base: Project = serde_json::from_value(fixture["page"]["items"][0].clone()).unwrap();
        for invalid in [
            serde_json::json!({"limit": 0}),
            serde_json::json!({"limit": 101}),
            serde_json::json!({"limit": 24, "cursor": null}),
            serde_json::json!({"limit": 24, "unknown": true}),
        ] {
            let request: Result<PageRequest, _> = serde_json::from_value(invalid);
            assert!(request.is_err() || request.unwrap().validate().is_err());
        }
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"name": null}),
            serde_json::json!({"unknown": true}),
            serde_json::json!({"tags": ["same", "same"]}),
        ] {
            let update: Result<UpdateProjectInput, _> = serde_json::from_value(invalid);
            if let Ok(update) = update {
                assert!(
                    base.apply_update(update, "2026-08-11T13:00:00Z".to_owned())
                        .is_err()
                );
            }
        }
        let mut create = fixture["createInput"].clone();
        create["name"] = serde_json::json!("");
        let input: CreateProjectInput = serde_json::from_value(create).unwrap();
        assert!(Project::create(input, "2026-08-11T12:00:00Z".to_owned()).is_err());
    }
}
