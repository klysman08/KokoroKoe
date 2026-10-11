import { create } from "zustand"

import { type ProjectId, type SessionId } from "@/contracts/projects"

export type MeetingFocus = { projectId: ProjectId; sessionId: SessionId }

type MeetingState = {
  /**
   * The meeting the Meeting view shows: the one just started, or a past one
   * opened from Home. Identity only — the session itself is always read back
   * from Rust, so this can never show a stale title or state.
   */
  focus: MeetingFocus | undefined
  open: (focus: MeetingFocus) => void
  clear: () => void
}

export const useMeetingStore = create<MeetingState>((set) => ({
  focus: undefined,
  open: (focus) => set({ focus }),
  clear: () => set({ focus: undefined }),
}))
