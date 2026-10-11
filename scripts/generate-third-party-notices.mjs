// Writes THIRD_PARTY_NOTICES.md: every third-party component the release
// installer distributes, with its license and the license texts it ships.
//
//   pnpm notices
//
// Rust crates are the normal (not dev or build) dependency closure of the
// application for x86_64-pc-windows-msvc, from `cargo metadata`; frontend
// packages are pnpm's production inventory. License texts are read from each
// package's own files and printed once per distinct text.
import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url))
const output = join(repositoryRoot, "THIRD_PARTY_NOTICES.md")
const licenseFile = /^(?:licen[cs]e|copying|notice|unlicense)(?:[-._].*)?$/iu

function run(command, args) {
  return process.platform === "win32"
    ? execFileSync(
        "cmd.exe",
        ["/d", "/s", "/c", [command, ...args].join(" ")],
        { cwd: repositoryRoot, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
      )
    : execFileSync(command, args, {
        cwd: repositoryRoot,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
      })
}

function licenseTexts(directory) {
  if (!existsSync(directory)) return []
  return readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && licenseFile.test(entry.name))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((entry) =>
      readFileSync(join(directory, entry.name), "utf8")
        .replace(/\r\n/gu, "\n")
        .trim(),
    )
    .filter(Boolean)
}

function rustCrates() {
  const metadata = JSON.parse(
    run("cargo", [
      "metadata",
      "--format-version",
      "1",
      "--locked",
      "--filter-platform",
      "x86_64-pc-windows-msvc",
      "--manifest-path",
      "src-tauri/Cargo.toml",
    ]),
  )
  const packages = new Map(metadata.packages.map((p) => [p.id, p]))
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]))
  const root = metadata.resolve.root
  const seen = new Set()
  const pending = [root]
  while (pending.length > 0) {
    const node = nodes.get(pending.pop())
    for (const dependency of node?.deps ?? []) {
      const normal = dependency.dep_kinds.some((kind) => kind.kind === null)
      if (!normal || seen.has(dependency.pkg)) continue
      seen.add(dependency.pkg)
      pending.push(dependency.pkg)
    }
  }
  return [...seen]
    .map((id) => packages.get(id))
    .map((p) => ({
      name: p.name,
      version: p.version,
      license: p.license ?? "see license text",
      url: p.repository ?? p.homepage ?? "",
      texts: licenseTexts(dirname(p.manifest_path)),
    }))
}

function frontendPackages() {
  const inventory = JSON.parse(
    run("pnpm", ["licenses", "list", "--prod", "--json"]),
  )
  return Object.values(inventory)
    .flat()
    .map((p) => ({
      name: p.name,
      version: p.versions.join(", "),
      license: p.license,
      url: p.homepage ?? "",
      texts: licenseTexts(p.paths[0]),
    }))
}

const byName = (a, b) =>
  a.name.localeCompare(b.name) || a.version.localeCompare(b.version)
const crates = rustCrates().sort(byName)
const frontend = frontendPackages().sort(byName)

// One copy of each distinct license text, with the packages that carry it.
const texts = new Map()
for (const item of [...crates, ...frontend]) {
  for (const text of item.texts) {
    const key = createHash("sha256").update(text).digest("hex")
    const entry = texts.get(key) ?? { text, packages: [] }
    entry.packages.push(`${item.name} ${item.version}`)
    texts.set(key, entry)
  }
}

// A few packages publish no license file; their declared SPDX license is
// linked to its standard text instead.
const withoutText = [...crates, ...frontend].filter(
  (item) => item.texts.length === 0,
)
const spdxLinks = (expression) =>
  [...new Set(expression.match(/[A-Za-z0-9.+-]+/gu) ?? [])]
    .filter((id) => !["OR", "AND", "WITH"].includes(id))
    .map((id) => `[${id}](https://spdx.org/licenses/${id}.html)`)
    .join(", ")

const missingTexts =
  withoutText.length === 0
    ? ""
    : [
        "These packages publish no license file; the standard text of their declared license applies:",
        "",
        ...withoutText.map(
          (item) =>
            `- ${item.name} ${item.version}: ${spdxLinks(item.license)}`,
        ),
        "",
        "",
      ].join("\n")

const escape = (value) => String(value).replaceAll("|", "\\|")
const table = (items) =>
  [
    "| Package | Version | License |",
    "| --- | --- | --- |",
    ...items.map(
      (item) =>
        `| ${item.url ? `[${escape(item.name)}](${item.url})` : escape(item.name)} | ${escape(item.version)} | ${escape(item.license)} |`,
    ),
  ].join("\n")
const fence = (text) => {
  const longest = Math.max(
    3,
    ...[...text.matchAll(/`+/gu)].map((m) => m[0].length + 1),
  )
  return `${"`".repeat(longest)}text\n${text}\n${"`".repeat(longest)}`
}

const whisperLicense = readFileSync(
  join(repositoryRoot, "scripts", "notices", "whisper.cpp-LICENSE.txt"),
  "utf8",
)
  .replace(/\r\n/gu, "\n")
  .trim()

const document = `# Third-party notices

KokoroKoe is MIT-licensed (see \`LICENSE\`). The installer also distributes the
third-party components below, each under its own license. This file is
generated by \`pnpm notices\`; do not edit it by hand.

## Native components

| Component | Version | License | Notes |
| --- | --- | --- | --- |
| [whisper.cpp and ggml](https://github.com/ggml-org/whisper.cpp) | 1.9.2 | MIT | \`whisper.dll\`, \`ggml*.dll\`, and the project's adapter, for CPU and Vulkan |
| Microsoft Visual C++ Runtime | 14.44 | Microsoft Visual Studio redistributable | \`msvcp140.dll\`, \`vcruntime140.dll\`, \`vcruntime140_1.dll\`, \`vcomp140.dll\`, deployed app-locally under the Visual Studio license terms |
| Microsoft Edge WebView2 | provided by Windows | Microsoft software license | Not distributed; the installer uses the copy on the system or downloads the official bootstrapper |

Whisper model weights are not distributed with the installer. Each is
downloaded on request from Hugging Face, verified by SHA-256, and published by
OpenAI under the MIT license.

### whisper.cpp and ggml

${fence(whisperLicense)}

## Rust crates (${crates.length})

${table(crates)}

## Frontend packages (${frontend.length})

${table(frontend)}

## License texts

Each distinct text appears once, followed by the packages that carry it.

${missingTexts}${[...texts.values()]
  .map(
    (entry) =>
      `### ${entry.packages.length === 1 ? entry.packages[0] : `${entry.packages[0]} and ${entry.packages.length - 1} more`}\n\nUsed by: ${entry.packages.join(", ")}\n\n${fence(entry.text)}`,
  )
  .join("\n\n")}
`

writeFileSync(output, document)
console.log(
  `THIRD_PARTY_NOTICES.md: ${crates.length} crates, ${frontend.length} frontend packages, ${texts.size} distinct license texts, ${Math.round(document.length / 1024)} KB`,
)
