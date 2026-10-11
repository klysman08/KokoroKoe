import { invoke } from "@tauri-apps/api/core"

import { toApplicationError } from "@/contracts/app-error"

export type ExternalPage = "repository" | "author"

export const EXTERNAL_PAGE_URLS: Record<ExternalPage, string> = {
  repository: "https://github.com/klysman08/KokoroKoe",
  author: "https://github.com/klysman08",
}

/**
 * Opens the repository or the author's profile in the default browser.
 *
 * Only the page's name crosses to Rust, which holds the addresses itself; the
 * URLs here are for the link's `href`, so hovering shows where it goes.
 */
export async function openExternalPage(page: ExternalPage): Promise<void> {
  try {
    await invoke("open_external_page", { page })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}
