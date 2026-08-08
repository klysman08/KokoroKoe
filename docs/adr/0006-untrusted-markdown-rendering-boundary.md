# ADR 0006: Untrusted Markdown Rendering Boundary

- Status: Accepted
- Date: 2026-08-08

## Context

Project, session, transcript, summary, and insight documents will eventually be portable Markdown. Their content can originate from users, local transcription, model output, or external edits and therefore cannot be trusted as HTML or executable UI content.

Rendering Markdown through an HTML string or exposing parser/plugin options to feature components would make later views easy to weaken accidentally. Images can also trigger unrequested network loads, and direct external links can navigate the Tauri WebView before a narrow desktop link-opening policy exists.

## Decision

All future Markdown-derived React UI uses the fixed `SanitizedMarkdown` boundary.

- Parse Markdown to React elements with `react-markdown` and GitHub-flavored syntax with `remark-gfm`.
- Ignore raw HTML with `skipHtml`; never add `rehype-raw`, MDX evaluation, or `dangerouslySetInnerHTML` to this boundary.
- Apply `rehype-sanitize` last with an explicit tag, attribute, and protocol schema.
- Exclude images and embedded or executable elements.
- Accept only absolute `http`, `https`, and `mailto` addresses after control-character validation; relative, protocol-relative, file, data, script, and custom schemes are rejected.
- Render all links as inert text until a separately authorized, user-gesture-driven Rust command is designed and tested.
- Keep filesystem reads, YAML front-matter validation, and document ownership in the future Rust Phase 4 persistence boundary.

The wrapper exposes only Markdown text and presentation class names. Feature code cannot replace its plugins, sanitizer, element mapping, or URL transform.

## Consequences

The application gains a reusable testable XSS and navigation boundary before it begins loading user documents. Raw HTML formatting, images, relative links, and active link opening are deliberately unavailable. Future work that needs one of those behaviors requires a new bounded security review and must not bypass this component.

Markdown parsing is synchronous. Phase 4 document services must enforce input-size limits and paginate or segment large content before it reaches React so untrusted files cannot monopolize the UI thread.
