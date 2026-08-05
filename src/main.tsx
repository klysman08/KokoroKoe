import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import App from "@/App"
import { reactRootErrorHandlers } from "@/app/errors/react-root-errors"
import "@/index.css"

const root = document.getElementById("root")

if (!root) {
  throw new Error("KokoroKoe could not find the application root")
}

createRoot(root, reactRootErrorHandlers).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
