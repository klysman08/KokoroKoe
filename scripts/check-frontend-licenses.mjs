import { execFileSync } from "node:child_process"

const output =
  process.platform === "win32"
    ? execFileSync(
        "cmd.exe",
        ["/d", "/s", "/c", "pnpm licenses list --prod --json"],
        { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
      )
    : execFileSync("pnpm", ["licenses", "list", "--prod", "--json"], {
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
      })
const inventory = JSON.parse(output)
const packageCount = Object.values(inventory).reduce(
  (count, packages) => count + packages.length,
  0,
)

if (packageCount === 0) {
  throw new Error("Frontend production license inventory is empty.")
}

console.log(
  `Frontend production license inventory parsed: ${packageCount} packages.`,
)
