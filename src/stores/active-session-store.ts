import { create } from "zustand"

import { type Session } from "@/contracts/projects"

export type ActiveSession = Pick<
  Session,
  "id" | "projectId" | "title" | "state" | "revision"
>

type ActiveSessionState = {
  activeSession: ActiveSession | undefined
  syncSession: (session: Session) => void
}

export const useActiveSessionStore = create<ActiveSessionState>((set) => ({
  activeSession: undefined,
  syncSession: (session) => {
    if (session.state === "transcribing" || session.state === "paused") {
      set({
        activeSession: {
          id: session.id,
          projectId: session.projectId,
          title: session.title,
          state: session.state,
          revision: session.revision,
        },
      })
    } else {
      set((current) =>
        current.activeSession?.id === session.id
          ? { activeSession: undefined }
          : current,
      )
    }
  },
}))
