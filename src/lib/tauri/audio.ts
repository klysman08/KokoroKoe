import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  audioDeviceListSchema,
  audioDeviceStatusChangedEnvelopeSchema,
  audioLevelUpdatedEnvelopeSchema,
  deviceTestInputSchema,
  deviceTestStatusSchema,
  type AudioDeviceList,
  type AudioDeviceStatusChangedEnvelope,
  type AudioLevelUpdatedEnvelope,
  type DeviceTestInput,
  type DeviceTestStatus,
} from "@/contracts/audio"
import { requestIdSchema, type RequestId } from "@/contracts/models"

export const AUDIO_LEVEL_UPDATED_EVENT = "audio-level-updated"
export const AUDIO_DEVICE_STATUS_CHANGED_EVENT = "audio-device-status-changed"

export async function listAudioDevices(): Promise<AudioDeviceList> {
  return invokeAndParse("list_audio_devices", audioDeviceListSchema)
}

export async function startAudioDeviceTest(
  input: DeviceTestInput,
  requestId: RequestId,
): Promise<DeviceTestStatus> {
  const validatedInput = deviceTestInputSchema.safeParse(input)
  const validatedRequestId = requestIdSchema.safeParse(requestId)
  if (!validatedInput.success || !validatedRequestId.success) {
    throw createRequestContractApplicationError()
  }
  const status = await invokeAndParse(
    "start_audio_device_test",
    deviceTestStatusSchema,
    { input: validatedInput.data, requestId: validatedRequestId.data },
  )
  if (
    status.requestId !== validatedRequestId.data ||
    status.source !== validatedInput.data.source
  ) {
    throw createContractApplicationError()
  }
  return status
}

export async function stopAudioDeviceTest(
  requestId: RequestId,
): Promise<DeviceTestStatus> {
  const validated = requestIdSchema.safeParse(requestId)
  if (!validated.success) {
    throw createRequestContractApplicationError()
  }
  const status = await invokeAndParse(
    "stop_audio_device_test",
    deviceTestStatusSchema,
    { requestId: validated.data },
  )
  if (status.requestId !== validated.data) {
    throw createContractApplicationError()
  }
  return status
}

export async function listenToAudioLevels(
  onEvent: (event: AudioLevelUpdatedEnvelope) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(AUDIO_LEVEL_UPDATED_EVENT, ({ payload }) => {
    const parsed = audioLevelUpdatedEnvelopeSchema.safeParse(payload)
    if (parsed.success) onEvent(parsed.data)
  })
}

export async function listenToAudioDeviceStatus(
  onEvent: (event: AudioDeviceStatusChangedEnvelope) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(AUDIO_DEVICE_STATUS_CHANGED_EVENT, ({ payload }) => {
    const parsed = audioDeviceStatusChangedEnvelopeSchema.safeParse(payload)
    if (parsed.success) onEvent(parsed.data)
  })
}

async function invokeAndParse<Output>(
  command: string,
  schema: {
    safeParse(
      value: unknown,
    ): { success: true; data: Output } | { success: false }
  },
  args?: Record<string, unknown>,
): Promise<Output> {
  let response: unknown
  try {
    response = await invoke<unknown>(command, args)
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const parsed = schema.safeParse(response)
  if (!parsed.success) throw createContractApplicationError()
  return parsed.data
}
