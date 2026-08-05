import { useState } from "react"

import { type ApplicationError } from "@/contracts/app-error"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { createSanitizedErrorReport } from "@/features/errors/sanitized-error-report"

type CopyStatus = "idle" | "copied" | "failed"

export function SanitizedErrorPanel({ error }: { error: ApplicationError }) {
  const [copyStatus, setCopyStatus] = useState<CopyStatus>("idle")

  const copyReport = async () => {
    try {
      await navigator.clipboard.writeText(createSanitizedErrorReport(error))
      setCopyStatus("copied")
    } catch {
      setCopyStatus("failed")
    }
  }

  return (
    <Card role="alert" className="border-destructive/40">
      <CardHeader>
        <CardTitle>Something needs attention</CardTitle>
        <CardDescription>{error.details.userMessage}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {error.details.technicalDetail && (
          <details className="text-muted-foreground text-sm">
            <summary className="text-foreground cursor-pointer font-medium">
              Technical details
            </summary>
            <p className="mt-2">{error.details.technicalDetail}</p>
          </details>
        )}
        <p className="text-muted-foreground text-xs">
          Reference: {error.details.correlationId}
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            onClick={() => void copyReport()}
            type="button"
            variant="outline"
          >
            Copy sanitized report
          </Button>
          <span aria-live="polite" className="text-muted-foreground text-xs">
            {copyStatus === "copied" && "Sanitized report copied."}
            {copyStatus === "failed" &&
              "Copy is unavailable. Select the visible details instead."}
          </span>
        </div>
      </CardContent>
    </Card>
  )
}
