import { ErrorBoundary } from "@/app/errors/ErrorBoundary"
import { AppProviders } from "@/app/providers/AppProviders"
import { AppRouter } from "@/app/router"

export default function App() {
  return (
    <ErrorBoundary>
      <AppProviders>
        <AppRouter />
      </AppProviders>
    </ErrorBoundary>
  )
}
