import { useQuery, useQueryClient } from "@tanstack/react-query"
import { KeyRound, RefreshCw, ShieldCheck, Trash2 } from "lucide-react"
import { useState, type FormEvent } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { ApplicationError } from "@/contracts/app-error"
import {
  openRouterApiKeySchema,
  type CredentialStatus,
} from "@/contracts/credentials"
import { type OpenRouterModel } from "@/contracts/openrouter"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  deleteOpenRouterApiKey,
  getOpenRouterCredentialStatus,
  setOpenRouterApiKey,
} from "@/lib/tauri/credentials"
import {
  listOpenRouterModels,
  validateOpenRouterApiKey,
} from "@/lib/tauri/openrouter"

const credentialQueryKey = ["openrouter-credential-status"] as const
const modelQueryKey = ["openrouter-models"] as const
const visibleModelLimit = 100

function pricePerMillion(value: string) {
  return new Intl.NumberFormat(undefined, {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 4,
  }).format(Number(value) * 1_000_000)
}

function contextTokens(value: number) {
  return new Intl.NumberFormat().format(value)
}

export function OpenRouterCredentialCard() {
  const queryClient = useQueryClient()
  const status = useQuery<CredentialStatus, ApplicationError>({
    queryKey: credentialQueryKey,
    queryFn: getOpenRouterCredentialStatus,
    gcTime: 0,
    retry: false,
  })
  const models = useQuery<OpenRouterModel[], ApplicationError>({
    queryKey: modelQueryKey,
    queryFn: () => listOpenRouterModels(false),
    enabled: status.data?.validatedAt !== undefined,
    staleTime: 15 * 60 * 1000,
    retry: false,
  })
  const [apiKey, setApiKey] = useState("")
  const [pending, setPending] = useState<
    "set" | "delete" | "validate" | "refresh"
  >()
  const [actionError, setActionError] = useState<ApplicationError>()
  const [saved, setSaved] = useState(false)
  const keyValid = openRouterApiKeySchema.safeParse(apiKey).success

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!keyValid || pending) return
    setPending("set")
    setActionError(undefined)
    setSaved(false)
    try {
      const next = await setOpenRouterApiKey(apiKey)
      queryClient.setQueryData(credentialQueryKey, next)
      queryClient.removeQueries({ queryKey: modelQueryKey })
      setSaved(true)
    } catch (error) {
      setActionError(error as ApplicationError)
    } finally {
      setApiKey("")
      setPending(undefined)
    }
  }

  async function remove() {
    if (pending) return
    setPending("delete")
    setActionError(undefined)
    setSaved(false)
    try {
      const next = await deleteOpenRouterApiKey()
      queryClient.setQueryData(credentialQueryKey, next)
      queryClient.removeQueries({ queryKey: modelQueryKey })
    } catch (error) {
      setActionError(error as ApplicationError)
    } finally {
      setApiKey("")
      setPending(undefined)
    }
  }

  async function validate() {
    if (pending || !status.data?.configured) return
    setPending("validate")
    setActionError(undefined)
    setSaved(false)
    try {
      const result = await validateOpenRouterApiKey()
      queryClient.setQueryData<CredentialStatus>(credentialQueryKey, {
        configured: true,
        validatedAt: result.validatedAt,
      })
      await queryClient.invalidateQueries({ queryKey: modelQueryKey })
    } catch (error) {
      setActionError(error as ApplicationError)
    } finally {
      setPending(undefined)
    }
  }

  async function refreshModels() {
    if (pending || !status.data?.validatedAt) return
    setPending("refresh")
    setActionError(undefined)
    try {
      const next = await listOpenRouterModels(true)
      queryClient.setQueryData(modelQueryKey, next)
    } catch (error) {
      setActionError(error as ApplicationError)
    } finally {
      setPending(undefined)
    }
  }

  const visibleModels = models.data?.slice(0, visibleModelLimit) ?? []

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <KeyRound aria-hidden="true" className="size-5" />
            <div>
              <CardTitle>OpenRouter credential</CardTitle>
              <CardDescription>
                Stored only in Windows Credential Manager for this Windows user.
              </CardDescription>
            </div>
          </div>
          {status.data && (
            <Badge variant={status.data.configured ? "secondary" : "outline"}>
              {status.data.validatedAt
                ? "Validated"
                : status.data.configured
                  ? "Configured"
                  : "Not configured"}
            </Badge>
          )}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <form onSubmit={submit}>
          <FieldGroup>
            <Field data-invalid={apiKey.length > 0 && !keyValid}>
              <FieldLabel htmlFor="openrouter-api-key">API key</FieldLabel>
              <Input
                id="openrouter-api-key"
                type="password"
                autoComplete="off"
                spellCheck={false}
                maxLength={2048}
                value={apiKey}
                disabled={pending !== undefined}
                aria-invalid={apiKey.length > 0 && !keyValid}
                aria-describedby="openrouter-api-key-help"
                onChange={(event) => setApiKey(event.currentTarget.value)}
              />
              <FieldDescription id="openrouter-api-key-help">
                The key is sent once to Rust, cleared from this field after the
                command, and never returned. Validation sends no transcript or
                audio.
              </FieldDescription>
              {apiKey.length > 0 && !keyValid && (
                <FieldError>
                  Enter the complete key without spaces or line breaks.
                </FieldError>
              )}
            </Field>
            <div className="flex flex-wrap gap-2">
              <Button
                type="submit"
                disabled={!keyValid || pending !== undefined}
              >
                {pending === "set" ? "Saving…" : "Save credential"}
              </Button>
              {status.data?.configured && (
                <Button
                  type="button"
                  variant="secondary"
                  disabled={pending !== undefined}
                  onClick={() => void validate()}
                >
                  <ShieldCheck aria-hidden="true" data-icon="inline-start" />
                  {pending === "validate"
                    ? "Validating…"
                    : "Validate credential"}
                </Button>
              )}
              {status.data?.configured && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={pending !== undefined}
                  onClick={remove}
                >
                  <Trash2 aria-hidden="true" data-icon="inline-start" />
                  {pending === "delete" ? "Removing…" : "Remove credential"}
                </Button>
              )}
            </div>
          </FieldGroup>
        </form>
        {saved && <p role="status">Credential saved securely.</p>}
        {status.isPending && <p role="status">Checking credential status…</p>}
        {status.data?.validatedAt && (
          <p className="text-muted-foreground text-sm">
            <span>Last validated: </span>
            <time dateTime={status.data.validatedAt}>
              {status.data.validatedAt}
            </time>
          </p>
        )}
        {status.data?.configured && !status.data.validatedAt && (
          <p className="text-muted-foreground text-sm">
            Stored locally, but not yet validated with OpenRouter.
          </p>
        )}
        {models.isPending && status.data?.validatedAt && (
          <p role="status">Loading privacy-filtered text models…</p>
        )}
        {models.data && (
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p className="font-medium">Available text models</p>
                <p className="text-muted-foreground text-sm">
                  Only models with a zero-data-retention endpoint are included.
                </p>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={pending !== undefined}
                onClick={() => void refreshModels()}
              >
                <RefreshCw aria-hidden="true" data-icon="inline-start" />
                {pending === "refresh" ? "Refreshing…" : "Refresh models"}
              </Button>
            </div>
            <Table>
              <TableCaption>
                Showing {visibleModels.length} of {models.data.length} validated
                catalog models. Prices are approximate USD per million tokens.
              </TableCaption>
              <TableHeader>
                <TableRow>
                  <TableHead>Model</TableHead>
                  <TableHead>Context</TableHead>
                  <TableHead>Input</TableHead>
                  <TableHead>Output</TableHead>
                  <TableHead>Capabilities</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {visibleModels.map((model) => (
                  <TableRow key={model.id}>
                    <TableCell className="max-w-72 whitespace-normal">
                      <p className="font-medium">{model.name}</p>
                      <p className="text-muted-foreground text-xs">
                        {model.provider} · {model.id}
                      </p>
                    </TableCell>
                    <TableCell>{contextTokens(model.contextLength)}</TableCell>
                    <TableCell>
                      {pricePerMillion(model.promptPricePerToken)}
                    </TableCell>
                    <TableCell>
                      {pricePerMillion(model.completionPricePerToken)}
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        <Badge variant="outline">Streaming</Badge>
                        <Badge variant="outline">ZDR</Badge>
                        {model.supportsStructuredOutputs && (
                          <Badge variant="outline">Structured</Badge>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
        {status.isError && <SanitizedErrorPanel error={status.error} />}
        {models.isError && <SanitizedErrorPanel error={models.error} />}
        {actionError && <SanitizedErrorPanel error={actionError} />}
      </CardContent>
    </Card>
  )
}
