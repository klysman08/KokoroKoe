import { execFileSync } from "node:child_process"
import { readFileSync, readdirSync } from "node:fs"
import { extname, join, relative } from "node:path"
import { fileURLToPath } from "node:url"

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url))

function fail(message) {
  throw new Error(`Repository policy check failed: ${message}`)
}

function read(relativePath) {
  return readFileSync(join(repositoryRoot, relativePath), "utf8")
}

const trackedFiles = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  {
    cwd: repositoryRoot,
    encoding: "utf8",
  },
)
  .split("\0")
  .filter(Boolean)

const forbiddenTrackedFiles = trackedFiles.filter((path) => {
  const normalized = path.replaceAll("\\", "/")
  const fileName = normalized.split("/").at(-1) ?? normalized
  const isSecretEnvironment =
    fileName === ".env" ||
    (fileName.startsWith(".env.") && fileName !== ".env.example")
  const isLocalArtifact =
    /\.(?:db|sqlite3?|wav|mp3|ggml|gguf|log)$/iu.test(fileName) ||
    /^ggml-.*\.bin$/iu.test(fileName) ||
    /(?:^|\/)(?:node_modules|dist|target|graphify-out|workspace)(?:\/|$)/u.test(
      normalized,
    )
  return isSecretEnvironment || isLocalArtifact
})

if (forbiddenTrackedFiles.length > 0) {
  fail(`forbidden tracked files: ${forbiddenTrackedFiles.join(", ")}`)
}

const workflow = read(".github/workflows/windows-ci.yml")
const requiredWorkflowFragments = [
  "permissions:\n  contents: read",
  "persist-credentials: false",
  "pnpm install --frozen-lockfile",
  "pnpm verify:frontend",
  "pnpm verify:rust",
  "pnpm audit:frontend",
  "pnpm licenses:frontend",
  "pnpm tauri build --ci --no-bundle --target x86_64-pc-windows-msvc -- --locked",
]

for (const fragment of requiredWorkflowFragments) {
  if (!workflow.includes(fragment)) {
    fail(`Windows CI is missing ${JSON.stringify(fragment)}`)
  }
}

if (
  /\bsecrets\s*\./iu.test(workflow) ||
  /openrouter|api[_-]?key/iu.test(workflow)
) {
  fail("Windows CI must not request or reference application secrets")
}

const usesLines = workflow
  .split(/\r?\n/u)
  .map((line) => line.trim())
  .filter((line) => line.startsWith("uses:"))

for (const line of usesLines) {
  if (!/@[0-9a-f]{40}(?:\s+#.*)?$/u.test(line)) {
    fail(`GitHub Action must be pinned to a full commit SHA: ${line}`)
  }
}

function sourceFiles(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name)
    if (entry.isDirectory()) return sourceFiles(path)
    return [path]
  })
}

const forbiddenDomInjectionApis = [
  "dangerouslySetInnerHTML",
  ".innerHTML",
  ".outerHTML",
  "insertAdjacentHTML",
  "document.write",
  "DOMParser",
]

const unsafeSource = sourceFiles(join(repositoryRoot, "src"))
  .filter((path) => [".js", ".jsx", ".ts", ".tsx"].includes(extname(path)))
  .filter((path) => !/\.(?:test|spec)\.[cm]?[jt]sx?$/u.test(path))
  .filter((path) => {
    const source = readFileSync(path, "utf8")
    return forbiddenDomInjectionApis.some((api) => source.includes(api))
  })
  .map((path) => relative(repositoryRoot, path))

if (unsafeSource.length > 0) {
  fail(`DOM injection API found in frontend source: ${unsafeSource.join(", ")}`)
}

const packageManifest = JSON.parse(read("package.json"))
if (
  packageManifest.dependencies?.["rehype-raw"] ||
  packageManifest.devDependencies?.["rehype-raw"]
) {
  fail("rehype-raw must not be added to this untrusted Markdown boundary")
}

console.log("Repository policy checks passed.")
