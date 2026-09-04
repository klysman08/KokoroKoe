use serde::{Deserialize, Serialize};

use super::{InsightType, PresetId, PresetSnapshot};

/// One entry of the closed built-in preset catalog.
///
/// A `Preset` is what the library shows while a preset is being *chosen*; a
/// [`PresetSnapshot`] is what a Session freezes once one has *been* chosen. The
/// extra fields here — the description, the built-in flag, the catalog
/// timestamps and revision — exist only to choose between presets and to
/// describe the library, so [`Preset::snapshot`] deliberately drops them rather
/// than widening what a Session record carries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Preset {
    pub(crate) schema_version: u32,
    pub(crate) id: PresetId,
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) built_in: bool,
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

#[cfg(test)]
mod tests {
    use super::Preset;

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
}
