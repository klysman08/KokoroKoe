import settingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import audioFixture from "../../../fixtures/contracts/audio-device-test-v1.json"
import presetFixture from "../../../fixtures/contracts/preset-catalog-v1.json"
import projectFixture from "../../../fixtures/contracts/project-management-v1.json"

import { audioDeviceListSchema } from "@/contracts/audio"
import { presetSchema } from "@/contracts/presets"
import { projectManagementFixtureSchema } from "@/contracts/projects"
import { appSettingsSchema } from "@/contracts/settings"
import {
  defaultMeetingLanguage,
  generalProjectInput,
  quickSessionInput,
} from "@/features/meeting/quick-start"

const settings = appSettingsSchema.parse({
  ...settingsFixture,
  defaultTranscriptionModelId: "whisper-large-v3-turbo-q5_0",
  defaultLlmModels: { insights: "settings/insights" },
  defaultSessionBudgetUsd: "0.50",
})
const devices = audioDeviceListSchema.parse(audioFixture.deviceList)
const preset = presetSchema.parse(presetFixture)
const project = {
  ...projectManagementFixtureSchema.parse(projectFixture).page.items[0]!,
  defaultTranscriptionModelId: "an-old-model",
  preferredLlmModels: { summaries: "project/summaries" },
}

describe("quick meeting start", () => {
  it("offers the system's language when it can, and English otherwise", () => {
    expect(defaultMeetingLanguage("pt-PT")).toBe("pt-BR")
    expect(defaultMeetingLanguage("es")).toBe("es-ES")
    expect(defaultMeetingLanguage("ko-KR")).toBe("en-US")
  })

  it("builds a complete session from the saved defaults", () => {
    const result = quickSessionInput({
      title: "  ",
      language: "pt-BR",
      preset,
      project,
      settings,
      devices,
    })
    if (!result.ok) throw new Error(result.reason)
    expect(result.input.title).toMatch(/^Meeting · /)
    // Settings names the model the user downloaded; an old project default
    // must not start a meeting on a model they have since removed.
    expect(result.input.transcriptionModelId).toBe(
      "whisper-large-v3-turbo-q5_0",
    )
    expect(result.input.llmModels).toEqual({
      insights: "settings/insights",
      summaries: "project/summaries",
      manualQuestions: undefined,
    })
    expect(result.input.spendingLimitUsd).toBe("0.50")
    // Default selections follow Windows if the headset changes before Start.
    expect(result.input.microphone.selection).toEqual({
      kind: "default",
      role: "communications",
    })
    expect(result.input.systemOutput.selection).toEqual({
      kind: "default",
      role: "console",
    })
    expect(result.input.preset.id).toBe(preset.id)
  })

  it("says which device is missing instead of starting without it", () => {
    expect(
      quickSessionInput({
        title: "Sync",
        language: "en-US",
        preset,
        project,
        settings,
        devices: { inputs: [], outputs: devices.outputs },
      }),
    ).toEqual({ ok: false, reason: "no_microphone" })
  })

  it("makes a General project from the saved defaults", () => {
    expect(generalProjectInput(settings)).toMatchObject({
      name: "General",
      defaultPresetId: settings.defaultPresetId,
      defaultTranscriptionModelId: settings.defaultTranscriptionModelId,
    })
  })
})
