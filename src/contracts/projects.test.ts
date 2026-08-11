import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/project-session-v1.json"
import {
  projectSchema,
  projectSessionFixtureSchema,
  sessionSchema,
} from "./projects"

describe("project and session contracts", () => {
  it("parses the shared project/session/folder fixture", () => {
    expect(projectSessionFixtureSchema.parse(fixture)).toEqual(fixture)
  })

  it.each([
    [{ ...fixture.project, unexpected: true }],
    [{ ...fixture.project, id: "not-a-uuid" }],
    [{ ...fixture.project, folderName: "escape/meeting" }],
    [{ ...fixture.project, preferredLlmModels: { insights: null } }],
    [{ ...fixture.project, tags: ["weekly", "weekly"] }],
    [{ ...fixture.project, revision: Number.MAX_SAFE_INTEGER + 1 }],
  ])("rejects an invalid project", (value) => {
    expect(() => projectSchema.parse(value)).toThrow()
  })

  it.each([
    [{ ...fixture.session, unexpected: true }],
    [{ ...fixture.session, language: "bad_language" }],
    [{ ...fixture.session, startedAt: null }],
    [{ ...fixture.session, state: "completed" }],
    [
      {
        ...fixture.session,
        preset: {
          ...fixture.session.preset,
          insightTypes: ["decision", "decision"],
        },
      },
    ],
    [{ ...fixture.session, folderName: "2026-08-10-demo--ffffffff" }],
    [
      {
        ...fixture.session,
        microphone: {
          ...fixture.session.microphone,
          selection: { kind: "fixed", endpointId: "different" },
        },
      },
    ],
  ])("rejects an invalid session", (value) => {
    expect(() => sessionSchema.parse(value)).toThrow()
  })

  it("rejects a mismatched project and derived layout", () => {
    expect(() =>
      projectSessionFixtureSchema.parse({
        ...fixture,
        session: {
          ...fixture.session,
          projectId: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        },
      }),
    ).toThrow()
    expect(() =>
      projectSessionFixtureSchema.parse({
        ...fixture,
        layout: {
          ...fixture.layout,
          transcriptDocument: "projects/other/transcript.md",
        },
      }),
    ).toThrow()
  })
})
