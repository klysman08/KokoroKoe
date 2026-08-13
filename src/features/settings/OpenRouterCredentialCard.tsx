import { useQuery, useQueryClient } from "@tanstack/react-query"
import { KeyRound, Trash2 } from "lucide-react"
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
import { ApplicationError } from "@/contracts/app-error"
import {
  openRouterApiKeySchema,
  type CredentialStatus,
} from "@/contracts/credentials"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  deleteOpenRouterApiKey,
  getOpenRouterCredentialStatus,
  setOpenRouterApiKey,
} from "@/lib/tauri/credentials"

const credentialQueryKey = ["openrouter-credential-status"] as const

export function OpenRouterCredentialCard() {
  const queryClient = useQueryClient()
  const status = useQuery<CredentialStatus, ApplicationError>({
    queryKey: credentialQueryKey,
    queryFn: getOpenRouterCredentialStatus,
    gcTime: 0,
    retry: false,
  })
  const [apiKey, setApiKey] = useState("")
  const [pending, setPending] = useState<"set" | "delete">()
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
    } catch (error) {
      setActionError(error as ApplicationError)
    } finally {
      setApiKey("")
      setPending(undefined)
    }
  }

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
              {status.data.configured ? "Configured" : "Not configured"}
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
                command, and never returned. Network validation is a later task.
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
        {status.isError && <SanitizedErrorPanel error={status.error} />}
        {actionError && <SanitizedErrorPanel error={actionError} />}
      </CardContent>
    </Card>
  )
}
