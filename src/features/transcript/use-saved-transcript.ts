import { useInfiniteQuery } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import { type ProjectId, type SessionId } from "@/contracts/projects"
import {
  type TranscriptPage,
  type TranscriptSearchPage,
} from "@/contracts/transcripts"
import { getTranscriptPage, searchTranscript } from "@/lib/tauri/transcripts"

const TRANSCRIPT_PAGE_SIZE = 50
const SEARCH_PAGE_SIZE = 20

export function useSavedTranscript(projectId: ProjectId, sessionId: SessionId) {
  return useInfiniteQuery<
    TranscriptPage,
    ApplicationError,
    { pages: TranscriptPage[]; pageParams: (string | undefined)[] },
    readonly ["saved-transcript", ProjectId, SessionId],
    string | undefined
  >({
    queryKey: ["saved-transcript", projectId, sessionId] as const,
    queryFn: ({ pageParam }) =>
      getTranscriptPage({
        projectId,
        sessionId,
        cursor: pageParam,
        limit: TRANSCRIPT_PAGE_SIZE,
      }),
    initialPageParam: undefined,
    getNextPageParam: (page) => page.nextCursor,
    maxPages: 10,
    gcTime: 0,
    retry: false,
  })
}

export function useTranscriptSearch(
  projectId: ProjectId,
  sessionId: SessionId,
  query: string,
) {
  return useInfiniteQuery<
    TranscriptSearchPage,
    ApplicationError,
    {
      pages: TranscriptSearchPage[]
      pageParams: (string | undefined)[]
    },
    readonly ["transcript-search", ProjectId, SessionId, string],
    string | undefined
  >({
    queryKey: ["transcript-search", projectId, sessionId, query] as const,
    queryFn: ({ pageParam }) =>
      searchTranscript({
        projectId,
        sessionId,
        query,
        cursor: pageParam,
        limit: SEARCH_PAGE_SIZE,
      }),
    initialPageParam: undefined,
    getNextPageParam: (page) => page.nextCursor,
    enabled: query.length > 0,
    maxPages: 10,
    gcTime: 0,
    retry: false,
  })
}
