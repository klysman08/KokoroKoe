use serde::{Deserialize, Serialize};

use super::{
    InsightType, PresetId, PresetSnapshot, ProjectId,
    projects::{
        JSON_SAFE_INTEGER_MAX, bounded_text, deserialize_non_null_optional, ordered_timestamps,
        parse_timestamp, portable_slug, valid_preset, valid_string_list,
    },
};

/// One entry of the preset library, built in or written by the user.
///
/// A `Preset` is what the library shows while a preset is being *chosen*; a
/// [`PresetSnapshot`] is what a Session freezes once one has *been* chosen. The
/// extra fields here — the description, the built-in flag, the owning project,
/// the timestamps and revision — exist only to choose between presets and to
/// manage them, so [`Preset::snapshot`] deliberately drops them rather than
/// widening what a Session record carries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Preset {
    pub(crate) schema_version: u32,
    pub(crate) id: PresetId,
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) built_in: bool,
    /// The project that owns this preset, absent for a built-in.
    ///
    /// ADR 0011 puts a custom preset inside the project folder it belongs to,
    /// so a project stays a self-explaining unit; a built-in belongs to the
    /// binary and to no project.
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_id: Option<ProjectId>,
    pub(crate) assistant_role: String,
    pub(crate) analysis_objectives: Vec<String>,
    pub(crate) insight_types: Vec<InsightType>,
    pub(crate) response_tone: String,
    pub(crate) final_summary_sections: Vec<String>,
    pub(crate) highlight_instructions: Vec<String>,
    pub(crate) prohibited_behaviors: Vec<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) revision: u64,
}

impl Preset {
    /// The frozen snapshot a Session copies when this preset is chosen.
    ///
    /// The snapshot carries the instructions that reach a prompt and nothing
    /// else: a Session that was started under this preset must keep reading the
    /// same way after the catalog moves on, which is why the copy is taken here
    /// instead of the Session storing an identifier to resolve later.
    ///
    /// The frontend takes this copy today, because `create_session` accepts a
    /// resolved snapshot. This is the Rust statement of the same rule, and what
    /// the catalog's validity test checks against.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn snapshot(&self) -> PresetSnapshot {
        PresetSnapshot {
            id: self.id.clone(),
            version: self.version,
            name: self.name.clone(),
            assistant_role: self.assistant_role.clone(),
            analysis_objectives: self.analysis_objectives.clone(),
            insight_types: self.insight_types.clone(),
            response_tone: self.response_tone.clone(),
            final_summary_sections: self.final_summary_sections.clone(),
            highlight_instructions: self.highlight_instructions.clone(),
            prohibited_behaviors: self.prohibited_behaviors.clone(),
        }
    }
}

/// A preset the user wrote, as it is stored in their own project folder.
///
/// This is the record the store reads and writes; [`Preset`] is what the
/// library publishes. They are kept apart because a stored preset always has a
/// project and a document name, and a published one may be a built-in that has
/// neither — an optional field in the record would make the store carry a case
/// that cannot occur.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CustomPreset {
    pub(crate) schema_version: u32,
    pub(crate) id: PresetId,
    pub(crate) project_id: ProjectId,
    pub(crate) document_name: String,
    /// The instruction version a Session freezes, distinct from `revision`.
    ///
    /// It moves only when the instructions themselves change, so renaming a
    /// preset or rewriting its description does not claim that meetings held
    /// under the previous version were run on different guidance.
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) assistant_role: String,
    pub(crate) analysis_objectives: Vec<String>,
    pub(crate) insight_types: Vec<InsightType>,
    pub(crate) response_tone: String,
    pub(crate) final_summary_sections: Vec<String>,
    pub(crate) highlight_instructions: Vec<String>,
    pub(crate) prohibited_behaviors: Vec<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreatePresetInput {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) assistant_role: String,
    pub(crate) analysis_objectives: Vec<String>,
    pub(crate) insight_types: Vec<InsightType>,
    pub(crate) response_tone: String,
    pub(crate) final_summary_sections: Vec<String>,
    pub(crate) highlight_instructions: Vec<String>,
    pub(crate) prohibited_behaviors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdatePresetInput {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) assistant_role: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) analysis_objectives: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) insight_types: Option<Vec<InsightType>>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_tone: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) final_summary_sections: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) highlight_instructions: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prohibited_behaviors: Option<Vec<String>>,
}

/// What an export wrote, and where the user chose to put it.
///
/// The path is here because the user picked it in a save dialog a moment
/// earlier; it is the one path the frontend may learn, exactly as the workspace
/// path is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PresetExport {
    pub(crate) path: String,
    pub(crate) files: u32,
    pub(crate) bytes: u64,
}

impl CustomPreset {
    /// The document name a preset's file is written under.
    ///
    /// Readable first and unique second, like every other portable name in the
    /// workspace: a slug of the preset's name so a person browsing the folder
    /// can tell what it is, and the identifier's first eight characters so two
    /// presets called "Weekly sync" cannot collide.
    pub(crate) fn derive_document_name(name: &str, id: &PresetId) -> String {
        format!("{}--{}.md", portable_slug(name, "preset"), id.suffix())
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.document_name != Self::derive_document_name_for(self)
            || !bounded_text(&self.description, 0, 1_024, true)
            || self.revision == 0
            || self.revision > JSON_SAFE_INTEGER_MAX
            || !ordered_timestamps(&self.created_at, &self.updated_at)
            || !valid_preset(&self.snapshot())
        {
            return Err("preset_contract_invalid");
        }
        Ok(())
    }

    fn derive_document_name_for(&self) -> String {
        Self::derive_document_name(&self.name, &self.id)
    }

    pub(crate) fn create(
        input: CreatePresetInput,
        project_id: ProjectId,
        created_at: String,
    ) -> Result<Self, &'static str> {
        parse_timestamp(&created_at).ok_or("preset_contract_invalid")?;
        let id = PresetId::generate();
        let value = Self {
            schema_version: 1,
            document_name: Self::derive_document_name(&input.name, &id),
            id,
            project_id,
            version: 1,
            name: input.name,
            description: input.description,
            assistant_role: input.assistant_role,
            analysis_objectives: input.analysis_objectives,
            insight_types: input.insight_types,
            response_tone: input.response_tone,
            final_summary_sections: input.final_summary_sections,
            highlight_instructions: input.highlight_instructions,
            prohibited_behaviors: input.prohibited_behaviors,
            created_at: created_at.clone(),
            updated_at: created_at,
            revision: 1,
        };
        value.validate()?;
        Ok(value)
    }

    /// A new preset seeded from an existing one, under a name of its own.
    ///
    /// The copy is a new preset in every way that matters — its own identifier,
    /// its own document, version one, revision one — because nothing about the
    /// original's history is true of it.
    pub(crate) fn duplicate_from(
        source: &Preset,
        name: String,
        project_id: ProjectId,
        created_at: String,
    ) -> Result<Self, &'static str> {
        Self::create(
            CreatePresetInput {
                name,
                description: source.description.clone(),
                assistant_role: source.assistant_role.clone(),
                analysis_objectives: source.analysis_objectives.clone(),
                insight_types: source.insight_types.clone(),
                response_tone: source.response_tone.clone(),
                final_summary_sections: source.final_summary_sections.clone(),
                highlight_instructions: source.highlight_instructions.clone(),
                prohibited_behaviors: source.prohibited_behaviors.clone(),
            },
            project_id,
            created_at,
        )
    }

    pub(crate) fn apply_update(
        &self,
        update: UpdatePresetInput,
        updated_at: String,
    ) -> Result<Self, &'static str> {
        if update.is_empty() {
            return Err("preset_update_invalid");
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= JSON_SAFE_INTEGER_MAX)
            .ok_or("preset_revision_exhausted")?;
        let requested = parse_timestamp(&updated_at).ok_or("preset_contract_invalid")?;
        let current = parse_timestamp(&self.updated_at).ok_or("preset_contract_invalid")?;

        let mut value = self.clone();
        value.name = update.name.unwrap_or_else(|| self.name.clone());
        value.description = update
            .description
            .unwrap_or_else(|| self.description.clone());
        value.assistant_role = update
            .assistant_role
            .unwrap_or_else(|| self.assistant_role.clone());
        value.analysis_objectives = update
            .analysis_objectives
            .unwrap_or_else(|| self.analysis_objectives.clone());
        value.insight_types = update
            .insight_types
            .unwrap_or_else(|| self.insight_types.clone());
        value.response_tone = update
            .response_tone
            .unwrap_or_else(|| self.response_tone.clone());
        value.final_summary_sections = update
            .final_summary_sections
            .unwrap_or_else(|| self.final_summary_sections.clone());
        value.highlight_instructions = update
            .highlight_instructions
            .unwrap_or_else(|| self.highlight_instructions.clone());
        value.prohibited_behaviors = update
            .prohibited_behaviors
            .unwrap_or_else(|| self.prohibited_behaviors.clone());

        // The instruction version moves only when the instructions do. A
        // Session records the version it froze, so bumping it for a renamed
        // preset would claim a difference in guidance that never happened.
        if value.snapshot_instructions() != self.snapshot_instructions() {
            value.version = self
                .version
                .checked_add(1)
                .ok_or("preset_revision_exhausted")?;
        }
        value.document_name = Self::derive_document_name(&value.name, &value.id);
        value.updated_at = if requested >= current {
            updated_at
        } else {
            self.updated_at.clone()
        };
        value.revision = next_revision;
        value.validate()?;
        Ok(value)
    }

    /// The published record, as the library shows it.
    pub(crate) fn into_preset(self) -> Preset {
        Preset {
            schema_version: self.schema_version,
            id: self.id,
            version: self.version,
            name: self.name,
            description: self.description,
            built_in: false,
            project_id: Some(self.project_id),
            assistant_role: self.assistant_role,
            analysis_objectives: self.analysis_objectives,
            insight_types: self.insight_types,
            response_tone: self.response_tone,
            final_summary_sections: self.final_summary_sections,
            highlight_instructions: self.highlight_instructions,
            prohibited_behaviors: self.prohibited_behaviors,
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision: self.revision,
        }
    }

    fn snapshot(&self) -> PresetSnapshot {
        PresetSnapshot {
            id: self.id.clone(),
            version: self.version,
            name: self.name.clone(),
            assistant_role: self.assistant_role.clone(),
            analysis_objectives: self.analysis_objectives.clone(),
            insight_types: self.insight_types.clone(),
            response_tone: self.response_tone.clone(),
            final_summary_sections: self.final_summary_sections.clone(),
            highlight_instructions: self.highlight_instructions.clone(),
            prohibited_behaviors: self.prohibited_behaviors.clone(),
        }
    }

    /// Everything a Session would freeze except the fields that only name it.
    fn snapshot_instructions(&self) -> PresetSnapshot {
        let mut snapshot = self.snapshot();
        snapshot.name = String::new();
        snapshot.version = 0;
        snapshot
    }
}

impl UpdatePresetInput {
    pub(crate) fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.assistant_role.is_none()
            && self.analysis_objectives.is_none()
            && self.insight_types.is_none()
            && self.response_tone.is_none()
            && self.final_summary_sections.is_none()
            && self.highlight_instructions.is_none()
            && self.prohibited_behaviors.is_none()
    }
}

impl CreatePresetInput {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !bounded_text(&self.description, 0, 1_024, true)
            || !valid_string_list(&self.analysis_objectives, 32, 1_024)
        {
            return Err("preset_contract_invalid");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CreatePresetInput, CustomPreset, InsightType, Preset, ProjectId, UpdatePresetInput,
    };

    #[test]
    fn the_published_preset_contract_is_exact() {
        let fixture = include_str!("../../../fixtures/contracts/preset-catalog-v1.json");
        let parsed: Preset = serde_json::from_str(fixture).expect("the fixture should parse");

        assert_eq!(parsed.schema_version, 1);
        assert!(parsed.built_in);
        assert_eq!(parsed.revision, 1);
        assert_eq!(parsed.snapshot().name, parsed.name);
        assert_eq!(parsed.snapshot().version, parsed.version);
    }

    #[test]
    fn the_published_preset_contract_rejects_unknown_and_null_fields() {
        let fixture = include_str!("../../../fixtures/contracts/preset-catalog-v1.json");
        let mut unknown: serde_json::Value =
            serde_json::from_str(fixture).expect("the fixture should parse");
        unknown["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Preset>(unknown).is_err());

        let mut nulled: serde_json::Value =
            serde_json::from_str(fixture).expect("the fixture should parse");
        nulled["description"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<Preset>(nulled).is_err());

        let mut snake_cased: serde_json::Value =
            serde_json::from_str(fixture).expect("the fixture should parse");
        snake_cased["built_in"] = snake_cased["builtIn"].clone();
        assert!(serde_json::from_value::<Preset>(snake_cased).is_err());
    }

    fn input() -> CreatePresetInput {
        CreatePresetInput {
            name: "Weekly sync".to_owned(),
            description: "Mine.".to_owned(),
            assistant_role: "Keep decisions visible.".to_owned(),
            analysis_objectives: vec!["Track decisions".to_owned()],
            insight_types: vec![InsightType::Decision],
            response_tone: "Concise.".to_owned(),
            final_summary_sections: vec!["Decisions".to_owned()],
            highlight_instructions: Vec::new(),
            prohibited_behaviors: Vec::new(),
        }
    }

    fn custom() -> CustomPreset {
        CustomPreset::create(input(), project_id(), "2026-09-04T10:00:00Z".to_owned())
            .expect("the input should be valid")
    }

    fn project_id() -> ProjectId {
        serde_json::from_value(serde_json::json!("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"))
            .expect("a valid project id")
    }

    #[test]
    fn a_new_custom_preset_starts_at_version_and_revision_one() {
        let preset = custom();

        assert_eq!(preset.version, 1);
        assert_eq!(preset.revision, 1);
        assert_eq!(preset.created_at, preset.updated_at);
        assert!(preset.document_name.starts_with("weekly-sync--"));
        assert!(preset.document_name.ends_with(".md"));
        assert!(!preset.clone().into_preset().built_in);
        assert_eq!(preset.clone().into_preset().project_id, Some(project_id()));
    }

    /// A Session records the instruction version it froze, so the version must
    /// move when the instructions do and stay put when only a label changes.
    #[test]
    fn the_instruction_version_moves_only_when_the_instructions_do() {
        let preset = custom();

        let renamed = preset
            .apply_update(
                UpdatePresetInput {
                    name: Some("Monthly review".to_owned()),
                    description: Some("Renamed.".to_owned()),
                    ..Default::default()
                },
                "2026-09-04T11:00:00Z".to_owned(),
            )
            .expect("a rename is a valid update");
        assert_eq!(renamed.version, 1);
        assert_eq!(renamed.revision, 2);
        assert!(renamed.document_name.starts_with("monthly-review--"));

        let rewritten = renamed
            .apply_update(
                UpdatePresetInput {
                    insight_types: Some(vec![InsightType::Risk]),
                    ..Default::default()
                },
                "2026-09-04T12:00:00Z".to_owned(),
            )
            .expect("changing the insight types is a valid update");
        assert_eq!(rewritten.version, 2);
        assert_eq!(rewritten.revision, 3);
    }

    #[test]
    fn an_empty_or_invalid_update_is_refused() {
        let preset = custom();

        assert_eq!(
            preset.apply_update(
                UpdatePresetInput::default(),
                "2026-09-04T11:00:00Z".to_owned()
            ),
            Err("preset_update_invalid")
        );
        assert_eq!(
            preset.apply_update(
                UpdatePresetInput {
                    insight_types: Some(Vec::new()),
                    ..Default::default()
                },
                "2026-09-04T11:00:00Z".to_owned(),
            ),
            Err("preset_contract_invalid")
        );
    }

    /// A copy is a new preset, not a second name for the original.
    #[test]
    fn a_duplicate_carries_the_instructions_and_none_of_the_history() {
        let source = custom().into_preset();
        let copy = CustomPreset::duplicate_from(
            &source,
            "Weekly sync (copy)".to_owned(),
            project_id(),
            "2026-09-05T10:00:00Z".to_owned(),
        )
        .expect("duplicating should be valid");

        assert_ne!(copy.id, source.id);
        assert_eq!(copy.version, 1);
        assert_eq!(copy.revision, 1);
        assert_eq!(copy.assistant_role, source.assistant_role);
        assert_eq!(copy.insight_types, source.insight_types);
        assert_eq!(copy.created_at, "2026-09-05T10:00:00Z");
    }
}
