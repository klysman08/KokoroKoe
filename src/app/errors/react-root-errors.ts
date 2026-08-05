import { type RootOptions } from "react-dom/client"

export const reactRootErrorHandlers: RootOptions = {
  onCaughtError: () => {
    // The visible ErrorBoundary owns the sanitized user-facing report.
  },
  onUncaughtError: () => {
    // Never forward raw exceptions or component stacks to the WebView console.
  },
  onRecoverableError: () => {
    // A future metrics sink may record only a generated sanitized envelope.
  },
}
