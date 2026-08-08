import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  audioDeviceListSchema,
  audioPrototypeStatusSchema,
  audioPrototypeStartRequestSchema,
  type AudioDeviceList,
  type AudioPrototypeStartRequest,
  type AudioPrototypeStatus,
} from "@/contracts/audio"

export async function listAudioDevices(): Promise<AudioDeviceList> {
  return invokeAndParse("list_audio_devices", audioDeviceListSchema)
}

export async function startAudioCapturePrototype(
  request: AudioPrototypeStartRequest,
): Promise<AudioPrototypeStatus> {
  const validated = audioPrototypeStartRequestSchema.safeParse(request)
  if (!validated.success) {
    throw createRequestContractApplicationError()
  }
  return invokeAndParse(
    "start_audio_capture_prototype",
    audioPrototypeStatusSchema,
    {
      request: validated.data,
    },
  )
}

export async function getAudioCapturePrototypeStatus(): Promise<AudioPrototypeStatus> {
  return invokeAndParse(
    "get_audio_capture_prototype_status",
    audioPrototypeStatusSchema,
  )
}

export async function stopAudioCapturePrototype(): Promise<AudioPrototypeStatus> {
  return invokeAndParse(
    "stop_audio_capture_prototype",
    audioPrototypeStatusSchema,
  )
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
  if (!parsed.success) {
    throw createContractApplicationError()
  }
  return parsed.data
}
