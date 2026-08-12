import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import {
  type ProjectId,
  type Session,
  type SessionCreateRequest,
  type SessionPage,
  type SessionUpdateRequest,
} from "@/contracts/projects"
import {
  type PausePersistedSessionRequest,
  type ResumePersistedSessionRequest,
  type StartPersistedSessionRequest,
  type StopPersistedSessionRequest,
} from "@/contracts/session-lifecycle"
import {
  createSession,
  listSessions,
  pausePersistedSession,
  resumePersistedSession,
  startPersistedSession,
  stopPersistedSession,
  updateSession,
} from "@/lib/tauri/sessions"

const SESSION_PAGE_SIZE = 12
export const sessionsQueryKey = (projectId: ProjectId) =>
  ["sessions", projectId] as const

export function useSessionsQuery(projectId: ProjectId, enabled: boolean) {
  const queryKey = sessionsQueryKey(projectId)
  return useInfiniteQuery<
    SessionPage,
    ApplicationError,
    { pages: SessionPage[]; pageParams: (string | undefined)[] },
    typeof queryKey,
    string | undefined
  >({
    queryKey,
    queryFn: ({ pageParam }) =>
      listSessions({ projectId, cursor: pageParam, limit: SESSION_PAGE_SIZE }),
    initialPageParam: undefined,
    getNextPageParam: (page) => page.nextCursor,
    enabled,
    gcTime: 0,
    retry: (failureCount, error) => error.details.retryable && failureCount < 1,
    retryDelay: 0,
  })
}

export function useCreateSessionMutation(projectId: ProjectId) {
  const queryClient = useQueryClient()
  return useMutation<Session, ApplicationError, SessionCreateRequest>({
    mutationFn: createSession,
    retry: false,
    gcTime: 0,
    onSettled: async () => {
      await queryClient.invalidateQueries({
        queryKey: sessionsQueryKey(projectId),
      })
    },
  })
}

export function useUpdateSessionMutation(projectId: ProjectId) {
  const queryClient = useQueryClient()
  return useMutation<Session, ApplicationError, SessionUpdateRequest>({
    mutationFn: updateSession,
    retry: false,
    gcTime: 0,
    onSettled: async () => {
      await queryClient.invalidateQueries({
        queryKey: sessionsQueryKey(projectId),
      })
    },
  })
}

type SessionLifecycleAction =
  | { kind: "start"; request: StartPersistedSessionRequest }
  | { kind: "pause"; request: PausePersistedSessionRequest }
  | { kind: "resume"; request: ResumePersistedSessionRequest }
  | { kind: "stop"; request: StopPersistedSessionRequest }

export function useSessionLifecycleMutation(projectId: ProjectId) {
  const queryClient = useQueryClient()
  return useMutation<Session, ApplicationError, SessionLifecycleAction>({
    mutationFn: async (action) => {
      switch (action.kind) {
        case "start":
          return await startPersistedSession(action.request)
        case "pause":
          return await pausePersistedSession(action.request)
        case "resume":
          return await resumePersistedSession(action.request)
        case "stop":
          return await stopPersistedSession(action.request)
      }
    },
    retry: false,
    gcTime: 0,
    onSettled: async () => {
      await queryClient.invalidateQueries({
        queryKey: sessionsQueryKey(projectId),
      })
    },
  })
}
