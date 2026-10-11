// Regenerates the README screenshots and the social preview from the real UI.
//
// Vite serves `scripts/screenshots/`, which renders the application against a
// mocked IPC layer and fictional data. Headless Microsoft Edge loads each view
// over the DevTools protocol, waits for the page to say the demo has played,
// and saves the picture. Nothing here touches a real workspace, transcript, or
// API key.
//
//   pnpm screenshots
//
// Set EDGE_PATH when Edge is not in its default location.
import { spawn } from "node:child_process"
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"

import { createServer } from "vite"

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url))
const outputDirectory = join(repositoryRoot, "docs", "assets")
const vitePort = 1431
const devToolsPort = 9333
const readyTimeoutMs = 30_000

const shots = [
  { file: "og.png", path: "og.html", width: 1200, height: 630 },
  {
    file: "screenshot-meeting.png",
    path: "index.html?view=meeting",
    width: 1440,
    height: 900,
  },
  {
    file: "screenshot-home.png",
    path: "index.html?view=home",
    width: 1440,
    height: 900,
  },
  {
    file: "screenshot-dashboard.png",
    path: "index.html?view=dashboard",
    width: 1440,
    height: 900,
  },
]

// The social preview holds the app in an iframe; either document may carry
// the flag the harness raises once its demo has played.
const readyExpression = `(() => {
  const frame = document.querySelector("iframe")
  const target = frame ? frame.contentDocument : document
  return target?.documentElement.dataset.screenshotReady === "true"
})()`

function findEdge() {
  const candidates = [
    process.env.EDGE_PATH,
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  ].filter(Boolean)
  const found = candidates.find((candidate) => existsSync(candidate))
  if (!found) throw new Error("Microsoft Edge was not found; set EDGE_PATH.")
  return found
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function pageTarget() {
  const deadline = Date.now() + readyTimeoutMs
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${devToolsPort}/json/list`)
      const page = (await response.json()).find((t) => t.type === "page")
      if (page) return page.webSocketDebuggerUrl
    } catch {
      // Edge is still starting.
    }
    await delay(250)
  }
  throw new Error("Edge did not expose a page over the DevTools protocol.")
}

function connect(url) {
  const socket = new WebSocket(url)
  const pending = new Map()
  let nextId = 1
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data)
    const waiter = pending.get(message.id)
    if (!waiter) return
    pending.delete(message.id)
    if (message.error) waiter.reject(new Error(message.error.message))
    else waiter.resolve(message.result)
  })
  const opened = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true })
    socket.addEventListener("error", reject, { once: true })
  })
  return opened.then(() => ({
    send(method, params = {}) {
      const id = nextId++
      socket.send(JSON.stringify({ id, method, params }))
      return new Promise((resolve, reject) =>
        pending.set(id, { resolve, reject }),
      )
    },
    close: () => socket.close(),
  }))
}

async function capture(devTools, shot) {
  await devTools.send("Emulation.setDeviceMetricsOverride", {
    width: shot.width,
    height: shot.height,
    deviceScaleFactor: 1,
    mobile: false,
  })
  await devTools.send("Page.navigate", {
    url: `http://localhost:${vitePort}/scripts/screenshots/${shot.path}`,
  })
  const deadline = Date.now() + readyTimeoutMs
  for (;;) {
    const { result } = await devTools.send("Runtime.evaluate", {
      expression: readyExpression,
      returnByValue: true,
    })
    if (result.value === true) break
    if (Date.now() > deadline)
      throw new Error(`${shot.file} never became ready`)
    await delay(250)
  }
  const { data } = await devTools.send("Page.captureScreenshot", {
    format: "png",
  })
  const output = join(outputDirectory, shot.file)
  writeFileSync(output, Buffer.from(data, "base64"))
  console.log(`${shot.file}  ${Math.round((data.length * 3) / 4 / 1024)} KB`)
}

mkdirSync(outputDirectory, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), "kokorokoe-screenshots-"))
const server = await createServer({
  root: repositoryRoot,
  logLevel: "error",
  server: { port: vitePort, strictPort: true, hmr: false },
})
await server.listen()
const edge = spawn(
  findEdge(),
  [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    "--no-first-run",
    "--lang=en-US",
    `--user-data-dir=${profile}`,
    `--remote-debugging-port=${devToolsPort}`,
    "about:blank",
  ],
  { stdio: "ignore" },
)

try {
  const devTools = await connect(await pageTarget())
  try {
    for (const shot of shots) await capture(devTools, shot)
  } finally {
    devTools.close()
  }
} finally {
  edge.kill()
  await server.close()
  await delay(500)
  rmSync(profile, { recursive: true, force: true, maxRetries: 5 })
}
