import { fireEvent, render, screen } from "@testing-library/react"

import { SanitizedMarkdown } from "./SanitizedMarkdown"
import { sanitizeMarkdownUrl } from "./markdown-security"

describe("SanitizedMarkdown", () => {
  it("renders the supported CommonMark and GFM subset", () => {
    render(
      <SanitizedMarkdown>{`# Session summary

**Decision:** ship the local build.

- [x] Verify tests
- Keep *audio local*

| Owner | Task |
| --- | --- |
| You | Review |`}</SanitizedMarkdown>,
    )

    expect(
      screen.getByRole("heading", { name: "Session summary" }),
    ).toBeInTheDocument()
    expect(screen.getByText("Decision:").tagName).toBe("STRONG")
    expect(screen.getByText("audio local").tagName).toBe("EM")
    expect(screen.getByRole("table")).toBeInTheDocument()
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument()
  })

  it("drops raw HTML, executable elements, and remote images", () => {
    const { container } = render(
      <SanitizedMarkdown>{`Before

<script>globalThis.compromised = true</script>
<style>body { display: none }</style>
<svg><a href="javascript:alert(1)">unsafe svg</a></svg>
<object data="https://example.test/object"></object>
<iframe src="https://example.test/frame"></iframe>
<form><input name="secret" formaction="https://example.test/form"></form>
<img src="https://example.test/pixel" onerror="alert(1)" style="display:none">

![tracking pixel](https://example.test/track.png)

After`}</SanitizedMarkdown>,
    )

    expect(screen.getByText("Before")).toBeInTheDocument()
    expect(screen.getByText("After")).toBeInTheDocument()
    expect(
      container.querySelector(
        "script, style, svg, object, iframe, form, input, img",
      ),
    ).toBeNull()
    expect(container.innerHTML).not.toContain("example.test")
    expect(container.innerHTML).not.toContain("onerror")
  })

  it("renders links inertly and never navigates the document", () => {
    const originalLocation = window.location.href
    const { container } = render(
      <SanitizedMarkdown>{`[safe](https://example.com/path) [unsafe](javascript:alert(1))`}</SanitizedMarkdown>,
    )

    expect(container.querySelector("a")).toBeNull()
    expect(screen.getByText("safe")).toHaveAttribute(
      "data-markdown-link",
      "allowed",
    )
    expect(screen.getByText("unsafe")).toHaveAttribute(
      "data-markdown-link",
      "blocked",
    )

    fireEvent.click(screen.getByText("safe"))
    expect(window.location.href).toBe(originalLocation)
  })

  it("allows only the documented external address schemes", () => {
    expect(sanitizeMarkdownUrl("https://example.com/path")).toBe(
      "https://example.com/path",
    )
    expect(sanitizeMarkdownUrl("http://example.com/path")).toBe(
      "http://example.com/path",
    )
    expect(sanitizeMarkdownUrl("mailto:owner@example.com")).toBe(
      "mailto:owner@example.com",
    )
    expect(sanitizeMarkdownUrl("javascript:alert(1)")).toBeUndefined()
    expect(sanitizeMarkdownUrl("JaVaScRiPt:alert(1)")).toBeUndefined()
    expect(sanitizeMarkdownUrl("vbscript:alert(1)")).toBeUndefined()
    expect(sanitizeMarkdownUrl("file:///C:/meeting.md")).toBeUndefined()
    expect(sanitizeMarkdownUrl("data:text/html,unsafe")).toBeUndefined()
    expect(sanitizeMarkdownUrl("//example.com/path")).toBeUndefined()
    expect(sanitizeMarkdownUrl("/relative/path")).toBeUndefined()
    expect(sanitizeMarkdownUrl("https://example.com\nunsafe")).toBeUndefined()
  })
})
