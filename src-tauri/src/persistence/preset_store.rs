use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::domain::{CustomPreset, PresetId, ProjectId};

use super::project_store::{
    ProjectLocator, ProjectStore, ProjectStoreError, atomic_publish_new,
    atomic_replace_with_backup, open_snapshot_without_write_share,
};

const PRESETS_DIRECTORY: &str = "presets";
const TEMP_PRESET_DOCUMENT: &str = ".preset.md.tmp";
const BACKUP_PRESET_SUFFIX: &str = ".bak";
const MAX_PRESET_DOCUMENT_BYTES: u64 = 64 * 1024;
/// A person writes presets by hand, so this bounds a folder, not a workload.
const MAX_PRESETS_PER_PROJECT: usize = 64;
const MAX_DIRECTORY_ENTRIES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresetStoreError {
    pub(crate) code: &'static str,
}

impl PresetStoreError {
    pub(crate) fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for PresetStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for PresetStoreError {}

/// Project-store failures reach the caller as preset failures.
///
/// The two codes that carry meaning are kept; everything else becomes one
/// workspace failure, because a preset operation cannot explain a project
/// problem in terms the user is asking about.
impl From<ProjectStoreError> for PresetStoreError {
    fn from(error: ProjectStoreError) -> Self {
        match error.code {
            "project_snapshot_missing" | "project_not_found" => Self::new("preset_project_missing"),
            "project_workspace_invalid" => Self::new("preset_workspace_invalid"),
            _ => Self::new("preset_workspace_failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresetFingerprint([u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresetRecord {
    pub(crate) preset: CustomPreset,
    pub(crate) fingerprint: PresetFingerprint,
}

/// Reads and writes the custom presets inside one project folder.
///
/// ADR 0011 puts them there, so every operation is reached through the pinned
/// project directory the project store already knows how to open: a preset is
/// never addressed by a path the caller supplies, and a document that names a
/// different project is refused on read rather than trusted because of where it
/// was found.
pub(crate) struct PresetStore {
    projects: ProjectStore,
}

impl PresetStore {
    pub(crate) fn open(workspace_path: &Path) -> Result<Self, PresetStoreError> {
        Ok(Self {
            projects: ProjectStore::open(workspace_path)?,
        })
    }

    pub(crate) fn list(
        &self,
        locator: &ProjectLocator,
        project_id: ProjectId,
    ) -> Result<Vec<CustomPreset>, PresetStoreError> {
        let directory = self.presets_directory(locator)?;
        let Some(directory) = directory else {
            return Ok(Vec::new());
        };

        let mut names = Vec::new();
        let reader = fs::read_dir(&directory)
            .map_err(|_| PresetStoreError::new("preset_discovery_failed"))?;
        for entry in reader.take(MAX_DIRECTORY_ENTRIES) {
            let entry = entry.map_err(|_| PresetStoreError::new("preset_discovery_failed"))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            // A temporary or backup document is this store's own scaffolding,
            // and anything else in the folder is not ours to interpret.
            if !name.ends_with(".md") || name.starts_with('.') {
                continue;
            }
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                names.push(name);
            }
        }
        names.sort();

        let mut presets = Vec::new();
        for name in names.into_iter().take(MAX_PRESETS_PER_PROJECT) {
            // One unreadable document must not hide the rest of the folder: it
            // is skipped, exactly as project discovery skips a folder it cannot
            // parse, so a hand-edited file cannot take the library down.
            if let Ok(record) = self.read_document(&directory, &name)
                && record.preset.project_id == project_id
                && record.preset.document_name == name
            {
                presets.push(record.preset);
            }
        }
        presets.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(presets)
    }

    pub(crate) fn read(
        &self,
        locator: &ProjectLocator,
        project_id: ProjectId,
        preset_id: &PresetId,
    ) -> Result<PresetRecord, PresetStoreError> {
        let directory = self
            .presets_directory(locator)?
            .ok_or_else(|| PresetStoreError::new("preset_not_found"))?;
        let name = self.document_name_of(&directory, project_id, preset_id)?;
        self.read_document(&directory, &name)
    }

    pub(crate) fn create(
        &self,
        locator: &ProjectLocator,
        preset: &CustomPreset,
    ) -> Result<(), PresetStoreError> {
        preset
            .validate()
            .map_err(|_| PresetStoreError::new("preset_contract_invalid"))?;
        let project = self.projects.open_existing_project(locator)?;
        let directory = project.path().join(PRESETS_DIRECTORY);
        create_directory(&directory)?;
        project.revalidate()?;

        let document = render_preset_document(preset)?;
        let temporary = directory.join(TEMP_PRESET_DOCUMENT);
        let destination = directory.join(&preset.document_name);
        if destination.exists() {
            return Err(PresetStoreError::new("preset_already_exists"));
        }
        write_synced_temporary(&temporary, document.as_bytes())?;
        project.revalidate()?;
        // Publishing without REPLACE_EXISTING fails closed if another writer
        // created this document between the check above and here, so a create
        // can never overwrite a preset it did not write.
        let published = atomic_publish_new(&temporary, &destination)
            .map_err(|_| PresetStoreError::new("preset_publish_failed"));
        if published.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        published
    }

    pub(crate) fn update(
        &self,
        locator: &ProjectLocator,
        updated: &CustomPreset,
        expected_revision: u64,
        expected_fingerprint: PresetFingerprint,
    ) -> Result<PresetRecord, PresetStoreError> {
        updated
            .validate()
            .map_err(|_| PresetStoreError::new("preset_contract_invalid"))?;
        let project = self.projects.open_existing_project(locator)?;
        let directory = project.path().join(PRESETS_DIRECTORY);
        let current_name = self.document_name_of(&directory, updated.project_id, &updated.id)?;
        let current = self.read_document(&directory, &current_name)?;

        if current.preset.revision != expected_revision {
            return Err(PresetStoreError::new("preset_revision_conflict"));
        }
        if current.fingerprint != expected_fingerprint {
            return Err(PresetStoreError::new("preset_external_modification"));
        }
        if current.preset.id != updated.id
            || current.preset.project_id != updated.project_id
            || current.preset.created_at != updated.created_at
        {
            return Err(PresetStoreError::new("preset_identity_mismatch"));
        }
        if expected_revision.checked_add(1) != Some(updated.revision) {
            return Err(PresetStoreError::new("preset_revision_invalid"));
        }

        let document = render_preset_document(updated)?;
        let temporary = directory.join(TEMP_PRESET_DOCUMENT);
        let current_path = directory.join(&current_name);
        let destination = directory.join(&updated.document_name);
        let backup = directory.join(format!("{current_name}{BACKUP_PRESET_SUFFIX}"));

        write_synced_temporary(&temporary, document.as_bytes())?;
        project.revalidate()?;
        let result = atomic_replace_with_backup(&temporary, &current_path, &backup)
            .map_err(PresetStoreError::from);
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        // A renamed preset moves to the document its new name derives, so a
        // person browsing the folder still sees what the application shows.
        if destination != current_path && fs::rename(&current_path, &destination).is_err() {
            return Err(PresetStoreError::new("preset_publish_failed"));
        }
        let _ = fs::remove_file(&backup);
        project.revalidate()?;
        self.read_document(&directory, &updated.document_name)
    }

    /// Moves one preset out of the project, without destroying it.
    ///
    /// The same rule P6-018 applied to projects and sessions: the workspace is
    /// the user's, so removal relocates a file into `trash/` and never shreds
    /// one. Sessions are unaffected either way, because each froze its own copy.
    pub(crate) fn trash(
        &self,
        locator: &ProjectLocator,
        project_id: ProjectId,
        preset_id: &PresetId,
        expected_revision: u64,
    ) -> Result<PathBuf, PresetStoreError> {
        let source = {
            let project = self.projects.open_existing_project(locator)?;
            let directory = project.path().join(PRESETS_DIRECTORY);
            let name = self.document_name_of(&directory, project_id, preset_id)?;
            let current = self.read_document(&directory, &name)?;
            if current.preset.revision != expected_revision {
                return Err(PresetStoreError::new("preset_revision_conflict"));
            }
            directory.join(name)
        };
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| PresetStoreError::new("preset_trash_failed"))?
            .to_owned();
        let stem = file_name.strip_suffix(".md").unwrap_or(&file_name);
        let destination = self.projects.reserve_trash_path(
            &["projects", locator.folder_name(), PRESETS_DIRECTORY],
            stem,
        )?;
        let destination = destination.with_extension("md");
        fs::rename(&source, &destination)
            .map_err(|_| PresetStoreError::new("preset_trash_failed"))?;
        Ok(destination)
    }

    /// The preset's document exactly as it is stored, for export.
    ///
    /// Export copies the portable document rather than re-rendering it, so what
    /// leaves the workspace is the same file the workspace holds.
    pub(crate) fn read_document_bytes(
        &self,
        locator: &ProjectLocator,
        project_id: ProjectId,
        preset_id: &PresetId,
    ) -> Result<(CustomPreset, Vec<u8>), PresetStoreError> {
        let directory = self
            .presets_directory(locator)?
            .ok_or_else(|| PresetStoreError::new("preset_not_found"))?;
        let name = self.document_name_of(&directory, project_id, preset_id)?;
        let bytes = read_bounded(&directory.join(&name))?;
        let preset = parse_preset_document(&bytes)?;
        Ok((preset, bytes))
    }

    fn presets_directory(
        &self,
        locator: &ProjectLocator,
    ) -> Result<Option<PathBuf>, PresetStoreError> {
        let project = self.projects.open_existing_project(locator)?;
        let directory = project.path().join(PRESETS_DIRECTORY);
        project.revalidate()?;
        Ok(directory.is_dir().then_some(directory))
    }

    /// The document that holds one preset, found by reading the folder.
    ///
    /// The name is derived from the preset's own record rather than composed
    /// from a caller's string, so a request cannot name a file of its own.
    fn document_name_of(
        &self,
        directory: &Path,
        project_id: ProjectId,
        preset_id: &PresetId,
    ) -> Result<String, PresetStoreError> {
        let reader =
            fs::read_dir(directory).map_err(|_| PresetStoreError::new("preset_not_found"))?;
        for entry in reader.take(MAX_DIRECTORY_ENTRIES) {
            let entry = entry.map_err(|_| PresetStoreError::new("preset_discovery_failed"))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !name.ends_with(".md") || name.starts_with('.') {
                continue;
            }
            if let Ok(record) = self.read_document(directory, &name)
                && record.preset.id == *preset_id
                && record.preset.project_id == project_id
            {
                return Ok(name);
            }
        }
        Err(PresetStoreError::new("preset_not_found"))
    }

    fn read_document(
        &self,
        directory: &Path,
        name: &str,
    ) -> Result<PresetRecord, PresetStoreError> {
        let bytes = read_bounded(&directory.join(name))?;
        let preset = parse_preset_document(&bytes)?;
        if preset.document_name != name {
            return Err(PresetStoreError::new("preset_document_name_mismatch"));
        }
        Ok(PresetRecord {
            fingerprint: PresetFingerprint(Sha256::digest(&bytes).into()),
            preset,
        })
    }
}

fn create_directory(path: &Path) -> Result<(), PresetStoreError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => Err(PresetStoreError::new("preset_directory_create_failed")),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, PresetStoreError> {
    let mut file = open_snapshot_without_write_share(path)
        .map_err(|_| PresetStoreError::new("preset_not_found"))?;
    let length = file
        .metadata()
        .map_err(|_| PresetStoreError::new("preset_document_invalid"))?
        .len();
    if length > MAX_PRESET_DOCUMENT_BYTES {
        return Err(PresetStoreError::new("preset_document_invalid"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| PresetStoreError::new("preset_document_invalid"))?;
    Ok(bytes)
}

fn write_synced_temporary(path: &Path, bytes: &[u8]) -> Result<(), PresetStoreError> {
    let _ = fs::remove_file(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| PresetStoreError::new("preset_write_failed"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| PresetStoreError::new("preset_write_failed"))
}

/// The stored fields, in the order the document writes them.
const DOCUMENT_FIELDS: &[(&str, &str)] = &[
    ("schema_version", "schemaVersion"),
    ("document_type", "documentType"),
    ("id", "id"),
    ("project_id", "projectId"),
    ("document_name", "documentName"),
    ("version", "version"),
    ("name", "name"),
    ("description", "description"),
    ("assistant_role", "assistantRole"),
    ("analysis_objectives", "analysisObjectives"),
    ("insight_types", "insightTypes"),
    ("response_tone", "responseTone"),
    ("final_summary_sections", "finalSummarySections"),
    ("highlight_instructions", "highlightInstructions"),
    ("prohibited_behaviors", "prohibitedBehaviors"),
    ("created_at", "createdAt"),
    ("updated_at", "updatedAt"),
    ("revision", "revision"),
];

fn render_preset_document(preset: &CustomPreset) -> Result<String, PresetStoreError> {
    let json =
        serde_json::to_value(preset).map_err(|_| PresetStoreError::new("preset_render_failed"))?;
    let object = json
        .as_object()
        .ok_or_else(|| PresetStoreError::new("preset_render_failed"))?;

    let mut document = String::from("---\n");
    for (yaml_name, json_name) in DOCUMENT_FIELDS {
        document.push_str(yaml_name);
        document.push_str(": ");
        if *yaml_name == "document_type" {
            document.push_str("\"preset\"");
        } else {
            let value = object
                .get(*json_name)
                .ok_or_else(|| PresetStoreError::new("preset_render_failed"))?;
            document.push_str(&render_yaml_flow(value)?);
        }
        document.push('\n');
    }
    document.push_str("---\n");
    Ok(document)
}

fn render_yaml_flow(value: &serde_json::Value) -> Result<String, PresetStoreError> {
    match value {
        serde_json::Value::Array(items) => {
            let items = items
                .iter()
                .map(render_yaml_flow)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", items.join(", ")))
        }
        _ => {
            serde_json::to_string(value).map_err(|_| PresetStoreError::new("preset_render_failed"))
        }
    }
}

fn parse_preset_document(bytes: &[u8]) -> Result<CustomPreset, PresetStoreError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| PresetStoreError::new("preset_document_invalid"))?;
    let normalized = text.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(PresetStoreError::new("preset_document_invalid"));
    }
    let mut lines = normalized.lines();
    if lines.next() != Some("---") {
        return Err(PresetStoreError::new("preset_document_invalid"));
    }

    let mut object = serde_json::Map::new();
    for (yaml_name, json_name) in DOCUMENT_FIELDS {
        let line = lines
            .next()
            .ok_or_else(|| PresetStoreError::new("preset_document_invalid"))?;
        let (name, encoded) = line
            .split_once(": ")
            .ok_or_else(|| PresetStoreError::new("preset_document_invalid"))?;
        if name != *yaml_name {
            return Err(PresetStoreError::new("preset_document_invalid"));
        }
        let value: serde_json::Value = serde_json::from_str(encoded)
            .map_err(|_| PresetStoreError::new("preset_document_invalid"))?;
        object.insert((*json_name).to_owned(), value);
    }
    if lines.next() != Some("---") || lines.next().is_some() {
        return Err(PresetStoreError::new("preset_document_invalid"));
    }
    if object.remove("documentType") != Some(serde_json::json!("preset")) {
        return Err(PresetStoreError::new("preset_document_invalid"));
    }

    let preset: CustomPreset = serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|_| PresetStoreError::new("preset_document_invalid"))?;
    preset
        .validate()
        .map_err(|_| PresetStoreError::new("preset_document_invalid"))?;
    Ok(preset)
}

#[cfg(test)]
mod tests {
    use crate::domain::{CreatePresetInput, InsightType, Project};

    use super::super::project_store::{ProjectLocator, ProjectStore};
    use super::{CustomPreset, PresetStore, parse_preset_document, render_preset_document};

    fn project() -> Project {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["project"].clone()).unwrap()
    }

    fn input(name: &str) -> CreatePresetInput {
        CreatePresetInput {
            name: name.to_owned(),
            description: "A preset written by the user.".to_owned(),
            assistant_role: "Follow the conversation and keep decisions visible.".to_owned(),
            analysis_objectives: vec!["Track decisions".to_owned()],
            insight_types: vec![InsightType::Decision, InsightType::ActionItem],
            response_tone: "Concise.".to_owned(),
            final_summary_sections: vec!["Decisions".to_owned()],
            highlight_instructions: vec!["Highlight owners".to_owned()],
            prohibited_behaviors: vec!["Do not invent facts".to_owned()],
        }
    }

    fn preset(project: &Project, name: &str) -> CustomPreset {
        CustomPreset::create(input(name), project.id, "2026-09-04T10:00:00Z".to_owned()).unwrap()
    }

    #[test]
    fn a_preset_document_round_trips_through_strict_front_matter() {
        let document = render_preset_document(&preset(&project(), "Weekly sync")).unwrap();

        assert_eq!(
            parse_preset_document(document.as_bytes()).unwrap(),
            preset_with_same_identity(&document)
        );
        let crlf = document.replace('\n', "\r\n");
        assert_eq!(
            parse_preset_document(crlf.as_bytes()).unwrap(),
            preset_with_same_identity(&document)
        );
    }

    fn preset_with_same_identity(document: &str) -> CustomPreset {
        parse_preset_document(document.as_bytes()).unwrap()
    }

    /// A preset's fields are user text, and the front matter must stay a fixed
    /// shape no matter what is written into them.
    #[test]
    fn user_text_cannot_escape_the_front_matter_shape() {
        let mut value = preset(&project(), "Weekly sync");
        value.description = "---\nname: !!unsafe payload".to_owned();
        value.response_tone = "quoted: \"value\"\n# not a comment".to_owned();
        let document = render_preset_document(&value).unwrap();

        assert_eq!(document.matches("\n---\n").count(), 1);
        assert!(document.contains("description: \"---\\nname: !!unsafe payload\""));
        assert_eq!(parse_preset_document(document.as_bytes()).unwrap(), value);
    }

    #[test]
    fn the_parser_rejects_shape_and_document_type_changes() {
        let document = render_preset_document(&preset(&project(), "Weekly sync")).unwrap();
        for invalid in [
            document.replacen("name: ", "unknown: ", 1),
            document.replacen("document_type: \"preset\"", "document_type: \"project\"", 1),
            format!("{document}# trailing body\n"),
        ] {
            assert_eq!(
                parse_preset_document(invalid.as_bytes()).unwrap_err().code,
                "preset_document_invalid"
            );
        }
    }

    #[cfg(windows)]
    fn workspace_with_project() -> (tempfile::TempDir, Project, ProjectLocator) {
        let workspace = tempfile::tempdir().unwrap();
        let project = project();
        ProjectStore::open(workspace.path())
            .unwrap()
            .create_project(&project)
            .unwrap();
        let locator = ProjectLocator::from_project(&project).unwrap();
        (workspace, project, locator)
    }

    #[cfg(windows)]
    #[test]
    fn a_preset_is_written_inside_its_own_project_and_read_back() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let written = preset(&project, "Weekly sync");

        store.create(&locator, &written).unwrap();

        let document = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("presets")
            .join(&written.document_name);
        assert!(document.is_file());
        assert!(written.document_name.starts_with("weekly-sync--"));
        assert_eq!(
            store.list(&locator, project.id).unwrap(),
            vec![written.clone()]
        );
        assert_eq!(
            store
                .read(&locator, project.id, &written.id)
                .unwrap()
                .preset,
            written
        );
    }

    /// Writing never overwrites a document this call did not create.
    #[cfg(windows)]
    #[test]
    fn creating_the_same_preset_twice_is_refused() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let written = preset(&project, "Weekly sync");
        store.create(&locator, &written).unwrap();

        assert_eq!(
            store.create(&locator, &written).unwrap_err().code,
            "preset_already_exists"
        );
        assert_eq!(store.list(&locator, project.id).unwrap().len(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn a_project_lists_only_the_presets_that_name_it() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let mine = preset(&project, "Weekly sync");
        store.create(&locator, &mine).unwrap();

        // A document that names another project is ignored where it was found,
        // rather than adopted by the folder holding it.
        let mut foreign = mine.clone();
        foreign.id = crate::domain::PresetId::generate();
        foreign.project_id = other_project_id();
        foreign.document_name = CustomPreset::derive_document_name(&foreign.name, &foreign.id);
        let foreign_path = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("presets")
            .join(&foreign.document_name);
        std::fs::write(&foreign_path, render_preset_document(&foreign).unwrap()).unwrap();

        assert_eq!(store.list(&locator, project.id).unwrap(), vec![mine]);
        assert_eq!(
            store
                .read(&locator, project.id, &foreign.id)
                .unwrap_err()
                .code,
            "preset_not_found"
        );
    }

    #[cfg(windows)]
    fn other_project_id() -> crate::domain::ProjectId {
        serde_json::from_value(serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")).unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn an_edit_refuses_a_stale_revision_and_an_edited_document() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let written = preset(&project, "Weekly sync");
        store.create(&locator, &written).unwrap();
        let record = store.read(&locator, project.id, &written.id).unwrap();

        let updated = written
            .apply_update(
                crate::domain::UpdatePresetInput {
                    response_tone: Some("Warmer.".to_owned()),
                    ..Default::default()
                },
                "2026-09-04T11:00:00Z".to_owned(),
            )
            .unwrap();

        assert_eq!(
            store
                .update(&locator, &updated, 7, record.fingerprint)
                .unwrap_err()
                .code,
            "preset_revision_conflict"
        );

        let mut stale = record.fingerprint;
        stale = super::PresetFingerprint(stale.0.map(|byte| byte ^ 0xff));
        assert_eq!(
            store.update(&locator, &updated, 1, stale).unwrap_err().code,
            "preset_external_modification"
        );

        let saved = store
            .update(&locator, &updated, 1, record.fingerprint)
            .unwrap()
            .preset;
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.version, 2, "the instructions changed");
        assert_eq!(saved.response_tone, "Warmer.");
    }

    /// Renaming moves the document, so the folder keeps explaining itself.
    #[cfg(windows)]
    #[test]
    fn renaming_a_preset_moves_its_document_and_leaves_no_other_behind() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let written = preset(&project, "Weekly sync");
        store.create(&locator, &written).unwrap();
        let record = store.read(&locator, project.id, &written.id).unwrap();

        let renamed = written
            .apply_update(
                crate::domain::UpdatePresetInput {
                    name: Some("Monthly review".to_owned()),
                    ..Default::default()
                },
                "2026-09-04T11:00:00Z".to_owned(),
            )
            .unwrap();
        let saved = store
            .update(&locator, &renamed, 1, record.fingerprint)
            .unwrap()
            .preset;

        assert!(saved.document_name.starts_with("monthly-review--"));
        assert_eq!(saved.version, 1, "a rename is not a change of instructions");
        let presets_directory = workspace
            .path()
            .join("projects")
            .join(&project.folder_name)
            .join("presets");
        let documents = std::fs::read_dir(&presets_directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".md"))
            .collect::<Vec<_>>();
        assert_eq!(documents, vec![saved.document_name]);
    }

    /// Deletion relocates the document; nothing in this application shreds a
    /// file the user wrote.
    #[cfg(windows)]
    #[test]
    fn deleting_a_preset_moves_it_into_the_workspace_trash() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let written = preset(&project, "Weekly sync");
        store.create(&locator, &written).unwrap();

        assert_eq!(
            store
                .trash(&locator, project.id, &written.id, 9)
                .unwrap_err()
                .code,
            "preset_revision_conflict"
        );
        let moved = store.trash(&locator, project.id, &written.id, 1).unwrap();

        assert!(
            moved.starts_with(
                std::fs::canonicalize(workspace.path())
                    .unwrap()
                    .join("trash")
            )
        );
        assert!(moved.is_file());
        assert!(store.list(&locator, project.id).unwrap().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn an_export_reads_the_stored_document_rather_than_rendering_a_new_one() {
        let (workspace, project, locator) = workspace_with_project();
        let store = PresetStore::open(workspace.path()).unwrap();
        let written = preset(&project, "Weekly sync");
        store.create(&locator, &written).unwrap();

        let (preset, bytes) = store
            .read_document_bytes(&locator, project.id, &written.id)
            .unwrap();

        assert_eq!(preset, written);
        assert_eq!(
            bytes,
            std::fs::read(
                workspace
                    .path()
                    .join("projects")
                    .join(&project.folder_name)
                    .join("presets")
                    .join(&written.document_name)
            )
            .unwrap()
        );
    }
}
