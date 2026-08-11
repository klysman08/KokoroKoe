import { useEffect, useState, type ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { Mic, RefreshCw, Volume2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  type AudioDevice,
  type AudioSource,
  type DeviceRole,
  type DeviceSelection,
  type DeviceTestStatus,
} from "@/contracts/audio"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import { requestIdSchema, type RequestId } from "@/contracts/models"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  listAudioDevices,
  listenToAudioDeviceStatus,
  listenToAudioLevels,
  startAudioDeviceTest,
  stopAudioDeviceTest,
} from "@/lib/tauri/audio"

type TestView = {
  requestId: RequestId
  status: DeviceTestStatus["status"]
  health?: string
  peakDbfs?: number
}

const defaultSelections: Record<AudioSource, DeviceSelection> = {
  microphone: { kind: "default", role: "communications" },
  system_output: { kind: "default", role: "console" },
}

export function AudioDeviceSettings() {
  const devices = useQuery({
    queryKey: ["audio-devices"],
    queryFn: listAudioDevices,
    staleTime: 5_000,
  })
  const [selections, setSelections] = useState(defaultSelections)
  const [tests, setTests] = useState<Partial<Record<AudioSource, TestView>>>({})
  const [error, setError] = useState<ApplicationError | null>(null)

  useEffect(() => {
    let disposed = false
    let unlistenLevels: (() => void) | undefined
    let unlistenHealth: (() => void) | undefined
    void listenToAudioLevels((event) => {
      setTests((current) => {
        const test = current[event.payload.source]
        if (test?.requestId !== event.requestId) return current
        return {
          ...current,
          [event.payload.source]: {
            ...test,
            status: "active",
            health: test.health ?? "active",
            peakDbfs: event.payload.peakDbfs,
          },
        }
      })
    })
      .then((unlisten) => {
        if (disposed) unlisten()
        else unlistenLevels = unlisten
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    void listenToAudioDeviceStatus((event) => {
      setTests((current) => {
        const test = current[event.payload.source]
        if (test?.requestId !== event.requestId) return current
        return {
          ...current,
          [event.payload.source]: {
            ...test,
            health: event.payload.current.status,
            status:
              event.payload.current.status === "unavailable"
                ? "failed"
                : event.payload.current.status === "stopped"
                  ? "stopped"
                  : event.payload.current.status === "starting" ||
                      event.payload.current.status === "reconnecting"
                    ? "starting"
                    : "active",
          },
        }
      })
    })
      .then((unlisten) => {
        if (disposed) unlisten()
        else unlistenHealth = unlisten
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    return () => {
      disposed = true
      unlistenLevels?.()
      unlistenHealth?.()
    }
  }, [])

  async function start(source: AudioSource) {
    setError(null)
    const requestId = requestIdSchema.parse(globalThis.crypto.randomUUID())
    setTests((current) => ({
      ...current,
      [source]: { requestId, status: "starting" },
    }))
    try {
      const status = await startAudioDeviceTest(
        { source, selection: selections[source] },
        requestId,
      )
      setTests((current) => {
        const test = current[source]
        if (test?.requestId !== requestId) return current
        return {
          ...current,
          [source]: {
            ...test,
            status: test.status === "active" ? "active" : status.status,
          },
        }
      })
    } catch (caught) {
      setTests((current) => {
        if (current[source]?.requestId !== requestId) return current
        const next = { ...current }
        delete next[source]
        return next
      })
      setError(toApplicationError(caught))
    }
  }

  async function stop(source: AudioSource) {
    const test = tests[source]
    if (!test) return
    setError(null)
    try {
      const status = await stopAudioDeviceTest(test.requestId)
      setTests((current) => ({
        ...current,
        [source]: { ...test, status: status.status, health: "stopped" },
      }))
    } catch (caught) {
      setError(toApplicationError(caught))
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>Audio devices</CardTitle>
            <CardDescription className="mt-1 max-w-2xl leading-6">
              Choose a Windows default role or a fixed endpoint, then test the
              microphone and system output independently. Test audio is
              processed and discarded in bounded memory; only levels and device
              health reach this screen.
            </CardDescription>
          </div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={devices.isFetching}
            onClick={() => void devices.refetch()}
          >
            <RefreshCw aria-hidden="true" className="size-4" />
            Refresh devices
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-muted-foreground text-xs">
          These choices are temporary until a meeting session is created; this
          device-test task does not start or save a meeting.
        </p>
        <div className="grid gap-4 lg:grid-cols-2">
          <DeviceTestPanel
            source="microphone"
            title="Microphone input"
            icon={<Mic aria-hidden="true" className="size-4" />}
            devices={devices.data?.inputs ?? []}
            selection={selections.microphone}
            test={tests.microphone}
            disabled={devices.isPending}
            onSelection={(selection) =>
              setSelections((current) => ({
                ...current,
                microphone: selection,
              }))
            }
            onStart={() => void start("microphone")}
            onStop={() => void stop("microphone")}
          />
          <DeviceTestPanel
            source="system_output"
            title="System output"
            icon={<Volume2 aria-hidden="true" className="size-4" />}
            devices={devices.data?.outputs ?? []}
            selection={selections.system_output}
            test={tests.system_output}
            disabled={devices.isPending}
            onSelection={(selection) =>
              setSelections((current) => ({
                ...current,
                system_output: selection,
              }))
            }
            onStart={() => void start("system_output")}
            onStop={() => void stop("system_output")}
          />
        </div>
        {devices.isPending && (
          <p role="status" className="text-muted-foreground text-sm">
            Detecting Windows audio devicesâ€¦
          </p>
        )}
        {devices.isError && (
          <SanitizedErrorPanel error={toApplicationError(devices.error)} />
        )}
        {error && <SanitizedErrorPanel error={error} />}
      </CardContent>
    </Card>
  )
}

function DeviceTestPanel({
  source,
  title,
  icon,
  devices,
  selection,
  test,
  disabled,
  onSelection,
  onStart,
  onStop,
}: {
  source: AudioSource
  title: string
  icon: ReactNode
  devices: AudioDevice[]
  selection: DeviceSelection
  test: TestView | undefined
  disabled: boolean
  onSelection: (selection: DeviceSelection) => void
  onStart: () => void
  onStop: () => void
}) {
  const running = test && test.status !== "stopped" && test.status !== "failed"
  const meter = Math.max(
    0,
    Math.min(100, (((test?.peakDbfs ?? -60) + 60) / 60) * 100),
  )
  return (
    <section
      className="border-border space-y-3 rounded-lg border p-4"
      aria-label={title}
    >
      <div className="flex items-center justify-between gap-3">
        <h3 className="flex items-center gap-2 font-medium">
          {icon} {title}
        </h3>
        <Badge variant={running ? "secondary" : "outline"}>
          {test?.health ?? test?.status ?? "not tested"}
        </Badge>
      </div>
      <label className="block space-y-1 text-sm">
        <span className="font-medium">Device selection</span>
        <select
          className="border-input bg-background h-9 w-full rounded-md border px-3 text-sm"
          aria-label={`${title} device`}
          value={selectionValue(selection)}
          disabled={disabled || Boolean(running)}
          onChange={(event) => onSelection(parseSelection(event.target.value))}
        >
          <option value="default:console">Default console device</option>
          <option value="default:multimedia">Default multimedia device</option>
          <option value="default:communications">
            Default communications device
          </option>
          {devices.map((device) => (
            <option
              key={device.endpointId}
              value={`fixed:${device.endpointId}`}
            >
              {device.friendlyName}
            </option>
          ))}
        </select>
      </label>
      <div>
        <div
          className="bg-muted h-2 overflow-hidden rounded-full"
          aria-hidden="true"
        >
          <div
            className="bg-primary h-full transition-[width]"
            style={{ width: `${meter}%` }}
          />
        </div>
        <p className="text-muted-foreground mt-1 text-xs" aria-live="polite">
          {test?.peakDbfs === undefined
            ? "No level received"
            : `Peak ${test.peakDbfs.toFixed(1)} dBFS`}
        </p>
      </div>
      {running ? (
        <Button type="button" size="sm" variant="outline" onClick={onStop}>
          Stop {source === "microphone" ? "input" : "output"} test
        </Button>
      ) : (
        <Button type="button" size="sm" disabled={disabled} onClick={onStart}>
          Test {source === "microphone" ? "input" : "output"}
        </Button>
      )}
    </section>
  )
}

function selectionValue(selection: DeviceSelection): string {
  return selection.kind === "default"
    ? `default:${selection.role}`
    : `fixed:${selection.endpointId}`
}

function parseSelection(value: string): DeviceSelection {
  if (value.startsWith("fixed:")) {
    return { kind: "fixed", endpointId: value.slice("fixed:".length) }
  }
  return { kind: "default", role: value.slice("default:".length) as DeviceRole }
}
