import { useQueryClient } from "@tanstack/react-query"
import { useEffect, useState } from "react"

import { type ProjectId } from "@/contracts/projects"
import { type PersistenceStatus } from "@/contracts/session-lifecycle"
import {
  reduceLiveRecords,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import {
  listenToPersistenceStatus,
  listenToSessionTranscriptionFinals,
  listenToSessionTranscriptionGaps,
  listenToSessionTranscriptionPartials,
} from "@/lib/tauri/sessions"

/**
 * The live transcript of every running session in one project, keyed by
 * session id, as Rust publishes it.
 *
 * `finals` counts the finalized segments received per session while this view
 * was listening. Automatic insights use it to tell new speech from silence, so
 * a meeting where nobody has spoken since the last generation sends nothing.
 */
export function useSessionLiveRecords(projectId: ProjectId | undefined) {
  const queryClient = useQueryClient()
  const [records, setRecords] = useState<Record<string, TranscriptRecord[]>>({})
  const [finals, setFinals] = useState<Record<string, number>>({})
  const [persistence, setPersistence] = useState<
    Record<string, PersistenceStatus>
  >({})

  useEffect(() => {
    if (!projectId) return
    let disposed = false
    const unlisteners: Array<() => void> = []
    const append = (
      sessionId: string,
      event: Parameters<typeof reduceLiveRecords>[1],
    ) =>
      setRecords((current) => ({
        ...current,
        [sessionId]: reduceLiveRecords(current[sessionId] ?? [], event),
      }))
    for (const subscription of [
      listenToPersistenceStatus((status) => {
        if (status.projectId !== projectId) return
        setPersistence((current) => ({
          ...current,
          [status.sessionId]: status,
        }))
      }),
      listenToSessionTranscriptionFinals((envelope) => {
        if (envelope.projectId !== projectId) return
        append(envelope.sessionId, envelope.event)
        setFinals((current) => ({
          ...current,
          [envelope.sessionId]: (current[envelope.sessionId] ?? 0) + 1,
        }))
        void queryClient.invalidateQueries({
          queryKey: ["saved-transcript", projectId, envelope.sessionId],
        })
      }),
      listenToSessionTranscriptionPartials((envelope) => {
        if (envelope.projectId !== projectId) return
        append(envelope.sessionId, envelope.event)
      }),
      listenToSessionTranscriptionGaps((envelope) => {
        if (envelope.projectId !== projectId) return
        append(envelope.sessionId, envelope.event)
      }),
    ]) {
      void subscription
        .then((unlisten) => {
          if (disposed) unlisten()
          else unlisteners.push(unlisten)
        })
        .catch(() => undefined)
    }
    return () => {
      disposed = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [projectId, queryClient])

  return { records, finals, persistence }
}
