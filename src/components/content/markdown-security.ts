export function sanitizeMarkdownUrl(value: string): string | undefined {
  const containsControlCharacter = [...value].some((character) => {
    const codePoint = character.codePointAt(0) ?? 0
    return codePoint <= 0x1f || codePoint === 0x7f
  })

  if (!value || containsControlCharacter) {
    return undefined
  }

  try {
    const url = new URL(value)
    return ["http:", "https:", "mailto:"].includes(url.protocol)
      ? url.href
      : undefined
  } catch {
    return undefined
  }
}
