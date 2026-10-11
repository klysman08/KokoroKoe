import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import presetFixture from "../../../fixtures/contracts/preset-catalog-v1.json"
import projectFixture from "../../../fixtures/contracts/project-management-v1.json"

import { projectManagementFixtureSchema } from "@/contracts/projects"
import { ProjectPresets } from "@/features/presets/ProjectPresets"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const project =
  projectManagementFixtureSchema.parse(projectFixture).page.items[0]!
const custom = {
  ...presetFixture,
  id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
  name: "Weekly sync",
  description: "Mine.",
  builtIn: false,
  projectId: project.id,
  version: 3,
  revision: 4,
}

function renderPresets() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <ProjectPresets onClose={() => undefined} project={project} />
    </QueryClientProvider>,
  )
}

describe("ProjectPresets", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_presets") return [presetFixture, custom]
      if (command === "create_preset") return custom
      if (command === "update_preset") return { ...custom, revision: 5 }
      if (command === "duplicate_preset") return custom
      if (command === "delete_custom_preset") return null
      if (command === "export_preset")
        return {
          path: "C:\\Users\\Example\\weekly-sync.md",
          files: 1,
          bytes: 512,
        }
      throw new Error(`Unexpected command: ${command}`)
    })
  })

  it("reads the library of the project it was opened for", async () => {
    renderPresets()

    expect(await screen.findByText("Weekly sync")).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledWith("list_presets", {
      projectId: project.id,
    })
  })

  /// A built-in is a compile-time constant, so the surface offers no way to
  /// change one — duplicating is the route to a preset of your own.
  it("offers no edit or delete for a built-in preset", async () => {
    renderPresets()

    expect(
      await screen.findByRole("button", {
        name: `Duplicate ${presetFixture.name}`,
      }),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: `Edit ${presetFixture.name}` }),
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: `Delete ${presetFixture.name}` }),
    ).not.toBeInTheDocument()
    expect(
      screen.getByRole("button", { name: "Edit Weekly sync" }),
    ).toBeInTheDocument()
  })

  it("writes a new preset into this project", async () => {
    const user = userEvent.setup()
    renderPresets()

    await user.click(await screen.findByRole("button", { name: "New preset" }))
    await user.type(
      screen.getByRole("textbox", { name: "Preset name" }),
      "Standup",
    )
    await user.type(
      screen.getByRole("textbox", { name: "Response tone" }),
      "Brief.",
    )
    await user.type(
      screen.getByRole("textbox", { name: "Assistant role" }),
      "Track what is blocked.",
    )
    await user.type(
      screen.getByRole("textbox", { name: /^Analysis objectives/ }),
      "Track blockers",
    )
    await user.type(
      screen.getByRole("textbox", { name: /^Final summary sections/ }),
      "Blockers",
    )
    await user.click(screen.getByRole("button", { name: "Save preset" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_preset", {
        projectId: project.id,
        value: {
          name: "Standup",
          description: "",
          assistantRole: "Track what is blocked.",
          analysisObjectives: ["Track blockers"],
          insightTypes: ["decision", "action_item"],
          responseTone: "Brief.",
          finalSummarySections: ["Blockers"],
          highlightInstructions: [],
          prohibitedBehaviors: [],
        },
      }),
    )
  })

  it("sends an edit with the revision it displayed", async () => {
    const user = userEvent.setup()
    renderPresets()

    await user.click(
      await screen.findByRole("button", { name: "Edit Weekly sync" }),
    )
    const tone = screen.getByRole("textbox", { name: "Response tone" })
    await user.clear(tone)
    await user.type(tone, "Warmer.")
    await user.click(screen.getByRole("button", { name: "Save preset" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "update_preset",
        expect.objectContaining({
          projectId: project.id,
          presetId: custom.id,
          expectedRevision: custom.revision,
        }),
      ),
    )
  })

  it("duplicates a built-in under a name of its own", async () => {
    const user = userEvent.setup()
    renderPresets()

    await user.click(
      await screen.findByRole("button", {
        name: `Duplicate ${presetFixture.name}`,
      }),
    )

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("duplicate_preset", {
        projectId: project.id,
        presetId: presetFixture.id,
        name: `${presetFixture.name} (copy)`,
      }),
    )
  })

  /// Deletion relocates the document, and the confirmation says so.
  it("confirms before moving a preset to the Trash", async () => {
    const user = userEvent.setup()
    renderPresets()

    await user.click(
      await screen.findByRole("button", { name: "Delete Weekly sync" }),
    )
    expect(invokeMock).not.toHaveBeenCalledWith(
      "delete_custom_preset",
      expect.anything(),
    )
    await user.click(screen.getByRole("button", { name: "Move to Trash" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_custom_preset", {
        projectId: project.id,
        presetId: custom.id,
        expectedRevision: custom.revision,
      }),
    )
  })

  it("reports where an export was written", async () => {
    const user = userEvent.setup()
    renderPresets()

    await user.click(
      await screen.findByRole("button", { name: "Export Weekly sync" }),
    )

    expect(
      await screen.findByText(
        /Exported to C:\\Users\\Example\\weekly-sync\.md/,
      ),
    ).toBeInTheDocument()
  })

  it("shows a refused operation as a sanitized panel", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_presets") return [presetFixture, custom]
      throw {
        error: {
          code: "preset_built_in_read_only",
          userMessage: "Built-in presets cannot be edited or deleted.",
          severity: "warning",
          retryable: false,
          correlationId: "11111111-1111-4111-8111-111111111111",
        },
      }
    })
    renderPresets()

    await user.click(
      await screen.findByRole("button", {
        name: `Duplicate ${presetFixture.name}`,
      }),
    )

    expect(
      await screen.findByText("Built-in presets cannot be edited or deleted."),
    ).toBeInTheDocument()
  })
})
