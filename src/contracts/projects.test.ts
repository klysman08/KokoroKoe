import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/project-session-v1.json"
import managementFixture from "../../fixtures/contracts/project-management-v1.json"
import {
  createProjectInputSchema,
  projectManagementFixtureSchema,
  projectPageRequestSchema,
  projectSchema,
  projectSessionFixtureSchema,
  projectUpdateRequestSchema,
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

describe("project management contracts", () => {
  it("parses the shared request and page fixture", () => {
    expect(projectManagementFixtureSchema.parse(managementFixture)).toEqual(
      managementFixture,
    )
  })

  it.each([
    [{ limit: 0 }],
    [{ limit: 101 }],
    [{ limit: 24, cursor: null }],
    [{ limit: 24, cursor: "wrong:cursor" }],
    [{ limit: 24, unknown: true }],
  ])("rejects an invalid page request", (value) => {
    expect(() => projectPageRequestSchema.parse(value)).toThrow()
  })

  it.each([
    [{ ...managementFixture.createInput, name: "" }],
    [{ ...managementFixture.createInput, tags: ["same", "same"] }],
    [
      {
        ...managementFixture.createInput,
        preferredLlmModels: { insights: null },
      },
    ],
    [{ ...managementFixture.createInput, unknown: true }],
  ])("rejects invalid create input", (value) => {
    expect(() => createProjectInputSchema.parse(value)).toThrow()
  })

  it.each([
    [{ ...managementFixture.updateRequest, value: {} }],
    [{ ...managementFixture.updateRequest, value: { name: null } }],
    [{ ...managementFixture.updateRequest, expectedRevision: -1 }],
    [{ ...managementFixture.updateRequest, unexpected: true }],
  ])("rejects invalid versioned update input", (value) => {
    expect(() => projectUpdateRequestSchema.parse(value)).toThrow()
  })
})
