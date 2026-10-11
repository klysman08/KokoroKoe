mod catalog;

use crate::domain::{InsightType, Preset, PresetId};

/// The checked-in shape of one built-in preset.
///
/// Built-in presets are data, not user records: they live in the binary as
/// `&'static str`, exactly like the transcription-model catalog, so the closed
/// set is reviewable in one file and cannot be widened at runtime. The
/// allocating [`Preset`] is produced only at the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PresetDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub assistant_role: &'static str,
    pub analysis_objectives: &'static [&'static str],
    pub insight_types: &'static [InsightType],
    pub response_tone: &'static str,
    pub final_summary_sections: &'static [&'static str],
    pub highlight_instructions: &'static [&'static str],
    pub prohibited_behaviors: &'static [&'static str],
}

impl PresetDescriptor {
    /// The published record for this built-in preset.
    ///
    /// # Panics
    ///
    /// Panics when the checked-in identifier is not a UUID. The catalog is a
    /// compile-time constant covered by `built_in_identifiers_are_unique_uuids`,
    /// so a failure here is a repository defect rather than a runtime input.
    fn to_preset(self) -> Preset {
        Preset {
            schema_version: 1,
            id: PresetId::from_catalog_id(self.id)
                .expect("checked-in built-in preset IDs must be valid UUIDs"),
            version: 1,
            name: self.name.to_owned(),
            description: self.description.to_owned(),
            built_in: true,
            project_id: None,
            assistant_role: self.assistant_role.to_owned(),
            analysis_objectives: owned(self.analysis_objectives),
            insight_types: self.insight_types.to_vec(),
            response_tone: self.response_tone.to_owned(),
            final_summary_sections: owned(self.final_summary_sections),
            highlight_instructions: owned(self.highlight_instructions),
            prohibited_behaviors: owned(self.prohibited_behaviors),
            created_at: catalog::PUBLISHED_AT.to_owned(),
            updated_at: catalog::PUBLISHED_AT.to_owned(),
            revision: 1,
        }
    }
}

fn owned(values: &'static [&'static str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Every built-in preset, in the order the library presents them.
pub(crate) fn list() -> Vec<Preset> {
    catalog::ALL
        .iter()
        .map(|descriptor| descriptor.to_preset())
        .collect()
}

/// Whether this identifier names a preset that ships inside the binary.
///
/// The write commands ask before they touch the workspace: a built-in has no
/// document to edit or remove, so a request naming one is answered without a
/// filesystem operation at all.
pub(crate) fn is_built_in(preset_id: &PresetId) -> bool {
    catalog::ALL.iter().any(|descriptor| {
        PresetId::from_catalog_id(descriptor.id).is_some_and(|id| id == *preset_id)
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, path::PathBuf};

    use super::{catalog, list};
    use crate::domain::{AppSettings, PresetId, valid_preset};

    #[test]
    fn the_catalog_is_the_closed_manifest_set() {
        let names = catalog::ALL
            .iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            [
                "Technical interview",
                "Behavioral interview",
                "Business meeting",
                "Sales meeting",
                "Product meeting",
                "Brainstorming",
                "Class or lecture",
                "Customer support",
                "Custom",
            ]
        );
    }

    #[test]
    fn built_in_identifiers_are_unique_uuids() {
        let mut seen = HashSet::new();
        for descriptor in catalog::ALL {
            assert!(
                PresetId::from_catalog_id(descriptor.id).is_some(),
                "{} must carry a UUID identifier",
                descriptor.name
            );
            assert!(
                seen.insert(descriptor.id),
                "{} must not reuse another preset's identifier",
                descriptor.name
            );
        }
    }

    /// The one property the rest of the application depends on: anything the
    /// library offers must survive being frozen into a Session. A preset that
    /// failed this would be selectable and then rejected at session creation.
    #[test]
    fn every_built_in_preset_is_a_valid_session_snapshot() {
        for preset in list() {
            assert!(
                valid_preset(&preset.snapshot()),
                "{} must be accepted as a frozen session preset",
                preset.name
            );
        }
    }

    #[test]
    fn every_built_in_preset_requests_distinct_insight_types() {
        for descriptor in catalog::ALL {
            assert!(!descriptor.insight_types.is_empty());
            for (index, insight) in descriptor.insight_types.iter().enumerate() {
                assert!(
                    !descriptor.insight_types[..index].contains(insight),
                    "{} must not request the same insight type twice",
                    descriptor.name
                );
            }
        }
    }

    #[test]
    fn listing_is_stable_and_carries_no_clock() {
        let first = list();
        let second = list();

        assert_eq!(first, second);
        assert_eq!(first.len(), catalog::ALL.len());
        for preset in first {
            assert!(preset.built_in);
            assert_eq!(preset.schema_version, 1);
            assert_eq!(preset.version, 1);
            assert_eq!(preset.revision, 1);
            assert_eq!(preset.created_at, catalog::PUBLISHED_AT);
            assert_eq!(preset.updated_at, catalog::PUBLISHED_AT);
        }
    }

    /// The fixture the frontend contract test parses is the real catalog entry,
    /// so the two sides cannot drift apart while both keep passing.
    #[test]
    fn the_shared_fixture_is_a_catalog_entry() {
        let fixture: crate::domain::Preset = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/preset-catalog-v1.json"
        ))
        .expect("the shared fixture should parse");

        assert!(
            list().contains(&fixture),
            "the shared preset fixture must stay identical to the catalog entry it publishes"
        );
    }

    /// The write commands ask this before they open the workspace, so it has to
    /// answer for every catalog entry and for nothing else.
    #[test]
    fn only_catalog_identifiers_are_built_in() {
        for preset in list() {
            assert!(
                super::is_built_in(&preset.id),
                "{} is built in",
                preset.name
            );
        }
        assert!(!super::is_built_in(
            &PresetId::from_catalog_id("00000000-0000-4000-8000-000000000099")
                .expect("a valid UUID")
        ));
        assert!(!super::is_built_in(&PresetId::generate()));
    }

    /// The shipped default has to name a preset that exists, or a first run
    /// would inherit an identifier the library cannot show.
    #[test]
    fn the_default_settings_preset_is_in_the_catalog() {
        let settings =
            AppSettings::foundation_defaults(PathBuf::from(r"C:\Users\Example\Documents"))
                .expect("foundation defaults should be valid");

        let matched = list()
            .into_iter()
            .find(|preset| preset.id == settings.default_preset_id)
            .expect("the default preset must exist in the catalog");
        assert_eq!(matched.name, "Technical interview");
    }
}
