import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/model-management-v1.json"

import {
  cancelModelDownload,
  downloadTranscriptionModel,
  listenToModelDownloadProgress,
  listTranscriptionModels,
} from "@/lib/tauri/models"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const listenMock = vi.mocked(listen)

beforeEach(() => {
  invokeMock.mockReset()
  listenMock.mockReset()
})

it("uses exact model command names and camel-case arguments", async () => {
  invokeMock
    .mockResolvedValueOnce([fixture.installation])
    .mockResolvedValueOnce(fixture.installation.downloadJob)
    .mockResolvedValueOnce(fixture.installation.downloadJob)

  await listTranscriptionModels()
  await downloadTranscriptionModel(fixture.installation.descriptor.id)
  await cancelModelDownload(fixture.installation.downloadJob.requestId)

  expect(invokeMock.mock.calls).toEqual([
    ["list_transcription_models", undefined],
    [
      "download_transcription_model",
      {
        modelId: fixture.installation.descriptor.id,
        requestId: expect.any(String),
      },
    ],
    [
      "cancel_model_download",
      { requestId: fixture.installation.downloadJob.requestId },
    ],
  ])
})

it("validates progress before exposing it to the UI", async () => {
  let listener: ((event: { payload: unknown }) => void) | undefined
  const stop = vi.fn()
  listenMock.mockImplementation(async (_event, callback) => {
    listener = callback as (event: { payload: unknown }) => void
    return stop
  })
  const received = vi.fn()
  await listenToModelDownloadProgress(received)
  listener?.({ payload: fixture.event })
  listener?.({ payload: { ...fixture.event, schemaVersion: 2 } })
  expect(received).toHaveBeenCalledOnce()
  expect(received).toHaveBeenCalledWith(fixture.event)
})
