const MINUTE_MS = 60_000
const HOUR_MS = 60 * MINUTE_MS

export function formatDuration(ms: number) {
  if (ms === 0) return "0 min"
  if (ms < MINUTE_MS) return "< 1 min"
  const hours = Math.floor(ms / HOUR_MS)
  const minutes = Math.floor((ms % HOUR_MS) / MINUTE_MS)
  if (hours === 0) return `${minutes} min`
  return minutes === 0 ? `${hours} h` : `${hours} h ${minutes} min`
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const

export function formatBytes(bytes: number) {
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024
    unit += 1
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1
  return `${value.toFixed(digits)} ${BYTE_UNITS[unit]}`
}

const integer = new Intl.NumberFormat("en-US")

export function formatCount(value: number) {
  return integer.format(value)
}

/**
 * The ledger keeps twelve decimals so sums stay exact; a person reads cents,
 * or a few more digits when the whole amount is below a dollar.
 */
export function formatUsd(canonical: string) {
  const value = Number(canonical)
  if (value === 0) return "$0.00"
  if (value < 0.0001) return "< $0.0001"
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: value < 1 ? 4 : 2,
  }).format(value)
}

export function formatDateTime(iso: string) {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(iso))
}
