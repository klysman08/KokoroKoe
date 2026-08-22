use std::{
    fmt,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use crate::domain::{ProjectId, Session, SessionId, SessionSummaryContent, SessionSummaryDocument};

use super::{
    project_store::{
        atomic_publish_new, atomic_replace_with_backup, open_snapshot_without_write_share,
    },
    session_store::{SessionLocator, SessionStore, SessionStoreError},
};

const SUMMARY_DOCUMENT: &str = "summary.md";
const TEMP_SUMMARY_DOCUMENT: &str = ".summary.md.tmp";
const BACKUP_SUMMARY_DOCUMENT: &str = "summary.md.bak";
const MAX_SUMMARY_BYTES: u64 = 4 * 1024 * 1024;
const FRONT_MATTER_LINES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SummaryStoreError {
    pub(crate) code: &'static str,
}

impl SummaryStoreError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for SummaryStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for SummaryStoreError {}

/// Everything a summary publication needs beyond the Session it belongs to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SummaryPublication<'a> {
    pub(crate) session: &'a Session,
    pub(crate) content: &'a SessionSummaryContent,
    pub(crate) generated_at: &'a str,
    pub(crate) model_id: &'a str,
    pub(crate) segments_considered: u32,
    pub(crate) segments_included: u32,
}

pub(crate) struct SummaryStore {
    sessions: SessionStore,
}

impl SummaryStore {
    pub(crate) fn open(workspace_path: &Path) -> Result<Self, SummaryStoreError> {
        Ok(Self {
            sessions: SessionStore::open(workspace_path).map_err(map_session_error)?,
        })
    }

    /// Reads a previously written `summary.md`, returning `None` when the Session
    /// has no summary yet. A malformed or oversized document is reported as an
    /// error instead of being silently replaced.
    pub(crate) fn read_summary(
        &self,
        locator: &SessionLocator,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<Option<SessionSummaryDocument>, SummaryStoreError> {
        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_session_error)?;
        let paths = SummaryPaths::new(directory.path());
        let bytes = match read_locked(&paths.document) {
            Ok(bytes) => bytes,
            Err(error) if error.code == "summary_document_missing" => return Ok(None),
            Err(error) => return Err(error),
        };
        parse_summary(&bytes, project_id, session_id).map(Some)
    }

    /// Atomically publishes `summary.md`, retaining the previous document as a
    /// `.bak` sibling and verifying the published bytes before returning.
    pub(crate) fn write_summary(
        &self,
        locator: &SessionLocator,
        publication: SummaryPublication<'_>,
    ) -> Result<SessionSummaryDocument, SummaryStoreError> {
        let markdown = render_summary_body(publication.content);
        let document = SessionSummaryDocument {
            schema_version: 1,
            project_id: publication.session.project_id,
            session_id: publication.session.id,
            generated_at: publication.generated_at.to_owned(),
            model_id: publication.model_id.to_owned(),
            markdown,
            segments_considered: publication.segments_considered,
            segments_included: publication.segments_included,
        };
        document
            .validate()
            .map_err(|_| SummaryStoreError::new("summary_document_invalid"))?;
        let bytes = render_document(&document)?;
        if bytes.len() as u64 > MAX_SUMMARY_BYTES {
            return Err(SummaryStoreError::new("summary_document_too_large"));
        }

        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_session_error)?;
        let paths = SummaryPaths::new(directory.path());
        let existed = paths.document.is_file();
        remove_if_file(&paths.temporary)?;
        write_synced_temporary(&paths.temporary, &bytes)?;
        let result = (|| {
            directory.revalidate().map_err(map_session_error)?;
            if existed {
                remove_if_file(&paths.backup)?;
                atomic_replace_with_backup(&paths.temporary, &paths.document, &paths.backup)
                    .map_err(|_| SummaryStoreError::new("summary_document_publish_failed"))?;
            } else {
                atomic_publish_new(&paths.temporary, &paths.document)
                    .map_err(|_| SummaryStoreError::new("summary_document_publish_failed"))?;
            }
            directory.revalidate().map_err(map_session_error)?;
            let published = read_locked(&paths.document)?;
            if published != bytes {
                return Err(SummaryStoreError::new("summary_document_verify_failed"));
            }
            Ok(document.clone())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&paths.temporary);
        }
        result
    }
}

struct SummaryPaths {
    document: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
}

impl SummaryPaths {
    fn new(directory: &Path) -> Self {
        Self {
            document: directory.join(SUMMARY_DOCUMENT),
            temporary: directory.join(TEMP_SUMMARY_DOCUMENT),
            backup: directory.join(BACKUP_SUMMARY_DOCUMENT),
        }
    }
}

fn read_locked(path: &Path) -> Result<Vec<u8>, SummaryStoreError> {
    let mut file: File = open_snapshot_without_write_share(path).map_err(|error| {
        SummaryStoreError::new(match error.code {
            "project_snapshot_missing" | "session_snapshot_missing" => "summary_document_missing",
            _ => "summary_document_read_failed",
        })
    })?;
    let length = file
        .metadata()
        .map_err(|_| SummaryStoreError::new("summary_document_read_failed"))?
        .len();
    if length > MAX_SUMMARY_BYTES {
        return Err(SummaryStoreError::new("summary_document_too_large"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| SummaryStoreError::new("summary_document_read_failed"))?;
    Ok(bytes)
}

/// Renders the portable document: canonical front matter per the Manifest, then
/// the human-readable summary body.
fn render_document(document: &SessionSummaryDocument) -> Result<Vec<u8>, SummaryStoreError> {
    let mut rendered = String::from("---\nschema_version: 1\ndocument_type: \"summary\"\n");
    rendered.push_str("project_id: ");
    rendered.push_str(&json_scalar(&document.project_id)?);
    rendered.push_str("\nsession_id: ");
    rendered.push_str(&json_scalar(&document.session_id)?);
    rendered.push_str("\ngenerated_at: ");
    rendered.push_str(&json_scalar(&document.generated_at)?);
    rendered.push_str("\nmodel_id: ");
    rendered.push_str(&json_scalar(&document.model_id)?);
    rendered.push_str("\nsegments_considered: ");
    rendered.push_str(&document.segments_considered.to_string());
    rendered.push_str("\nsegments_included: ");
    rendered.push_str(&document.segments_included.to_string());
    rendered.push_str("\n---\n");
    rendered.push_str(&document.markdown);
    Ok(rendered.into_bytes())
}

fn parse_summary(
    bytes: &[u8],
    project_id: ProjectId,
    session_id: SessionId,
) -> Result<SessionSummaryDocument, SummaryStoreError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| SummaryStoreError::new("summary_document_invalid"))?;
    if text.contains('\r') {
        return Err(SummaryStoreError::new("summary_document_invalid"));
    }
    let mut lines = text.split_inclusive('\n');
    let mut header = String::new();
    let mut header_lines = Vec::new();
    for _ in 0..FRONT_MATTER_LINES {
        let line = lines
            .next()
            .ok_or_else(|| SummaryStoreError::new("summary_document_invalid"))?;
        header.push_str(line);
        header_lines.push(line.trim_end_matches('\n'));
    }
    if header_lines[0] != "---"
        || header_lines[1] != "schema_version: 1"
        || header_lines[2] != "document_type: \"summary\""
        || !header_lines[3].starts_with("project_id: ")
        || !header_lines[4].starts_with("session_id: ")
        || !header_lines[5].starts_with("generated_at: ")
        || !header_lines[6].starts_with("model_id: ")
        || !header_lines[7].starts_with("segments_considered: ")
        || !header_lines[8].starts_with("segments_included: ")
        || header_lines[9] != "---"
    {
        return Err(SummaryStoreError::new("summary_document_invalid"));
    }
    let stored_project: ProjectId = json_value(&header_lines[3]["project_id: ".len()..])?;
    let stored_session: SessionId = json_value(&header_lines[4]["session_id: ".len()..])?;
    if stored_project != project_id || stored_session != session_id {
        return Err(SummaryStoreError::new("summary_document_identity_mismatch"));
    }
    let document = SessionSummaryDocument {
        schema_version: 1,
        project_id: stored_project,
        session_id: stored_session,
        generated_at: json_value(&header_lines[5]["generated_at: ".len()..])?,
        model_id: json_value(&header_lines[6]["model_id: ".len()..])?,
        markdown: text[header.len()..].to_owned(),
        segments_considered: header_lines[7]["segments_considered: ".len()..]
            .parse()
            .map_err(|_| SummaryStoreError::new("summary_document_invalid"))?,
        segments_included: header_lines[8]["segments_included: ".len()..]
            .parse()
            .map_err(|_| SummaryStoreError::new("summary_document_invalid"))?,
    };
    document
        .validate()
        .map_err(|_| SummaryStoreError::new("summary_document_invalid"))?;
    Ok(document)
}

/// Renders the Manifest-required `summary.md` sections as inert Markdown.
///
/// Generated text is untrusted, so every value is emitted as plain paragraph or
/// list content and never as a heading, link, or embedded structure.
fn render_summary_body(content: &SessionSummaryContent) -> String {
    let mut body = String::from("\n# Session summary\n\n## Executive summary\n\n");
    body.push_str(&inert_block(&content.executive_summary));
    push_list(&mut body, "Main topics", &content.main_topics);
    push_list(&mut body, "Decisions", &content.decisions);

    body.push_str("\n## Action items\n\n");
    if content.action_items.is_empty() {
        body.push_str("_None recorded._\n");
    } else {
        for item in &content.action_items {
            body.push_str("- ");
            body.push_str(&inert_line(&item.text));
            let owner = item.owner.as_deref().map(inert_line);
            let deadline = item.deadline.as_deref().map(inert_line);
            match (owner, deadline) {
                (Some(owner), Some(deadline)) => {
                    body.push_str(&format!(" (Owner: {owner}; Due: {deadline})"));
                }
                (Some(owner), None) => body.push_str(&format!(" (Owner: {owner})")),
                (None, Some(deadline)) => body.push_str(&format!(" (Due: {deadline})")),
                (None, None) => {}
            }
            body.push('\n');
        }
    }

    push_list(&mut body, "Risks", &content.risks);
    push_list(&mut body, "Open questions", &content.open_questions);
    push_list(&mut body, "Next steps", &content.next_steps);
    body
}

fn push_list(body: &mut String, heading: &str, values: &[String]) {
    body.push_str("\n## ");
    body.push_str(heading);
    body.push_str("\n\n");
    if values.is_empty() {
        body.push_str("_None recorded._\n");
        return;
    }
    for value in values {
        body.push_str("- ");
        body.push_str(&inert_line(value));
        body.push('\n');
    }
}

/// Collapses generated text to a single line and neutralizes Markdown structure
/// so stored content cannot forge headings, lists, or block syntax.
fn inert_line(value: &str) -> String {
    let collapsed = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\\', "\\\\");
    collapsed
        .chars()
        .flat_map(|character| {
            let escaped = matches!(
                character,
                '#' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '|' | '~'
            );
            escaped
                .then_some('\\')
                .into_iter()
                .chain(std::iter::once(character))
        })
        .collect()
}

fn inert_block(value: &str) -> String {
    let mut block = inert_line(value);
    block.push('\n');
    block
}

fn json_scalar<T: serde::Serialize>(value: &T) -> Result<String, SummaryStoreError> {
    serde_json::to_string(value).map_err(|_| SummaryStoreError::new("summary_document_invalid"))
}

fn json_value<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, SummaryStoreError> {
    serde_json::from_str(value).map_err(|_| SummaryStoreError::new("summary_document_invalid"))
}

fn write_synced_temporary(path: &Path, bytes: &[u8]) -> Result<(), SummaryStoreError> {
    use std::io::Write as _;

    let mut file = File::create(path)
        .map_err(|_| SummaryStoreError::new("summary_document_publish_failed"))?;
    file.write_all(bytes)
        .map_err(|_| SummaryStoreError::new("summary_document_publish_failed"))?;
    file.sync_all()
        .map_err(|_| SummaryStoreError::new("summary_document_publish_failed"))
}

fn remove_if_file(path: &Path) -> Result<(), SummaryStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SummaryStoreError::new("summary_document_publish_failed")),
    }
}

fn map_session_error(error: SessionStoreError) -> SummaryStoreError {
    SummaryStoreError::new(match error.code {
        "session_snapshot_missing" => "summary_session_missing",
        "workspace_required" => "workspace_required",
        _ => "summary_session_unavailable",
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use crate::{
        domain::{Project, SummaryActionItem},
        persistence::{ProjectStore, SessionStore},
    };

    use super::*;

    fn publication<'a>(
        session: &'a Session,
        content: &'a SessionSummaryContent,
        generated_at: &'a str,
        segments_considered: u32,
        segments_included: u32,
    ) -> SummaryPublication<'a> {
        SummaryPublication {
            session,
            content,
            generated_at,
            model_id: "example/text-model",
            segments_considered,
            segments_included,
        }
    }

    fn content() -> SessionSummaryContent {
        SessionSummaryContent {
            executive_summary: "The team kept the transcript local.".to_owned(),
            main_topics: vec!["Transcript storage".to_owned()],
            decisions: vec!["Keep the transcript local.".to_owned()],
            action_items: vec![SummaryActionItem {
                text: "Confirm the retention window.".to_owned(),
                owner: Some("Product lead".to_owned()),
                deadline: Some("2026-08-29T17:00:00Z".to_owned()),
            }],
            risks: vec![],
            open_questions: vec!["How long may audio be kept?".to_owned()],
            next_steps: vec!["Bring the proposal to review.".to_owned()],
        }
    }

    fn workspace() -> (
        tempfile::TempDir,
        Project,
        Session,
        SessionLocator,
        SummaryStore,
    ) {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        let project: Project = serde_json::from_value(fixture["project"].clone()).unwrap();
        let session: Session = serde_json::from_value(fixture["session"].clone()).unwrap();
        let root = tempdir().unwrap();
        ProjectStore::open(root.path())
            .unwrap()
            .create_project(&project)
            .unwrap();
        SessionStore::open(root.path())
            .unwrap()
            .create_session(&project, &session)
            .unwrap();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let store = SummaryStore::open(root.path()).unwrap();
        (root, project, session, locator, store)
    }

    #[test]
    fn an_absent_summary_reads_as_none_and_a_written_one_round_trips() {
        let (root, project, session, locator, store) = workspace();

        assert!(
            store
                .read_summary(&locator, project.id, session.id)
                .unwrap()
                .is_none()
        );

        let written = store
            .write_summary(
                &locator,
                publication(&session, &content(), "2026-08-22T18:30:00Z", 42, 40),
            )
            .unwrap();
        let read = store
            .read_summary(&locator, project.id, session.id)
            .unwrap()
            .unwrap();

        assert_eq!(read, written);
        assert_eq!(read.segments_considered, 42);
        assert_eq!(read.segments_included, 40);
        assert!(read.markdown.contains("## Decisions"));
        assert!(read.markdown.contains("Owner: Product lead"));
        assert!(read.markdown.contains("_None recorded._"));
        drop(root);
    }

    #[test]
    fn regeneration_replaces_the_document_and_retains_a_backup() {
        let (root, project, session, locator, store) = workspace();
        store
            .write_summary(
                &locator,
                publication(&session, &content(), "2026-08-22T18:30:00Z", 42, 40),
            )
            .unwrap();

        let mut second = content();
        second.executive_summary = "A regenerated summary.".to_owned();
        let replaced = store
            .write_summary(
                &locator,
                publication(&session, &second, "2026-08-22T19:00:00Z", 50, 48),
            )
            .unwrap();

        assert!(replaced.markdown.contains("A regenerated summary."));
        let read = store
            .read_summary(&locator, project.id, session.id)
            .unwrap()
            .unwrap();
        assert_eq!(read, replaced);
        let directory = SessionStore::open(root.path())
            .unwrap()
            .open_existing_session(&locator)
            .unwrap();
        assert!(SummaryPaths::new(directory.path()).backup.is_file());
    }

    #[test]
    fn generated_markdown_structure_cannot_be_forged_by_model_output() {
        let mut hostile = content();
        hostile.executive_summary =
            "# Injected heading\n\n<script>alert(1)</script> [link](http://example.com)".to_owned();
        hostile.decisions = vec!["- nested\n## fake heading".to_owned()];

        let body = render_summary_body(&hostile);

        assert!(!body.contains("\n# Injected heading"));
        assert!(!body.contains("\n## fake heading"));
        assert!(!body.contains("<script>"));
        assert!(body.contains("\\#"));
        assert!(body.contains("\\<script\\>"));
        assert_eq!(body.matches("\n# ").count(), 1);
        assert_eq!(body.matches("\n## ").count(), 7);
    }

    #[test]
    fn a_foreign_or_malformed_document_is_rejected_instead_of_returned() {
        let (root, project, session, locator, store) = workspace();
        store
            .write_summary(
                &locator,
                publication(&session, &content(), "2026-08-22T18:30:00Z", 42, 40),
            )
            .unwrap();
        let directory = SessionStore::open(root.path())
            .unwrap()
            .open_existing_session(&locator)
            .unwrap();
        let path = SummaryPaths::new(directory.path()).document;

        let foreign: ProjectId =
            serde_json::from_value(json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")).unwrap();
        assert_eq!(
            store
                .read_summary(&locator, foreign, session.id)
                .unwrap_err()
                .code,
            "summary_document_identity_mismatch"
        );

        std::fs::write(&path, b"not a summary document").unwrap();
        assert_eq!(
            store
                .read_summary(&locator, project.id, session.id)
                .unwrap_err()
                .code,
            "summary_document_invalid"
        );
    }
}
