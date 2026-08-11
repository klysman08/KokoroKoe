import {
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import {
  type CreateProjectInput,
  type Project,
  type ProjectPage,
  type ProjectUpdateRequest,
} from "@/contracts/projects"
import {
  createProject,
  listProjects,
  updateProject,
} from "@/lib/tauri/projects"

export const projectsQueryKey = ["projects"] as const
const PROJECT_PAGE_SIZE = 24

export function useProjectsQuery() {
  return useInfiniteQuery<
    ProjectPage,
    ApplicationError,
    { pages: ProjectPage[]; pageParams: (string | undefined)[] },
    typeof projectsQueryKey,
    string | undefined
  >({
    queryKey: projectsQueryKey,
    queryFn: ({ pageParam }) =>
      listProjects({ cursor: pageParam, limit: PROJECT_PAGE_SIZE }),
    initialPageParam: undefined,
    getNextPageParam: (page) => page.nextCursor,
    gcTime: 0,
    retry: (failureCount, error) => error.details.retryable && failureCount < 1,
    retryDelay: 0,
  })
}

export function useCreateProjectMutation() {
  const queryClient = useQueryClient()
  return useMutation<Project, ApplicationError, CreateProjectInput>({
    mutationFn: createProject,
    retry: false,
    gcTime: 0,
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: projectsQueryKey })
    },
  })
}

export function useUpdateProjectMutation() {
  const queryClient = useQueryClient()
  return useMutation<Project, ApplicationError, ProjectUpdateRequest>({
    mutationFn: updateProject,
    retry: false,
    gcTime: 0,
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: projectsQueryKey })
    },
  })
}
