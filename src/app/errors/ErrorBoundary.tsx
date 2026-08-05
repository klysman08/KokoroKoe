import { Component, type ReactNode } from "react"

import {
  ApplicationError,
  createUnexpectedApplicationError,
} from "@/contracts/app-error"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"

type ErrorBoundaryProps = {
  children: ReactNode
}

type ErrorBoundaryState = {
  error?: ApplicationError
}

export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = {}

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { error: createUnexpectedApplicationError() }
  }

  componentDidCatch() {
    // Raw errors and component stacks can contain local paths or content.
    // A future local metrics sink may record only the sanitized envelope.
  }

  render() {
    if (this.state.error) {
      return (
        <main className="bg-background text-foreground grid min-h-svh place-items-center p-6">
          <div className="w-full max-w-xl">
            <SanitizedErrorPanel error={this.state.error} />
          </div>
        </main>
      )
    }

    return this.props.children
  }
}
