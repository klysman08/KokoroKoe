import "@testing-library/jest-dom/vitest"

// The jsdom build used here exposes no Web Storage, so `globalThis.localStorage`
// is undefined and every persistence assertion would silently pass or crash.
// The real WebView2 runtime always provides it, so tests get a minimal
// in-memory implementation rather than skipping the behavior.
if (typeof globalThis.localStorage === "undefined") {
  const entries = new Map<string, string>()
  const storage: Storage = {
    get length() {
      return entries.size
    },
    clear: () => entries.clear(),
    getItem: (key) => entries.get(String(key)) ?? null,
    key: (index) => [...entries.keys()][index] ?? null,
    removeItem: (key) => {
      entries.delete(String(key))
    },
    setItem: (key, value) => {
      entries.set(String(key), String(value))
    },
  }
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: storage,
    writable: false,
  })
}
