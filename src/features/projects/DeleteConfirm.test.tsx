import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { DeleteConfirm } from "./DeleteConfirm"

describe("DeleteConfirm", () => {
  /// One click must never remove anything: the first press only asks.
  it("asks before it removes", async () => {
    const user = userEvent.setup()
    const onDelete = vi.fn()
    render(
      <DeleteConfirm
        busy={false}
        label="Delete project Planning"
        onDelete={onDelete}
        what="project"
      />,
    )

    await user.click(
      screen.getByRole("button", { name: "Delete project Planning" }),
    )

    expect(onDelete).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: /move to trash/i }))
    expect(onDelete).toHaveBeenCalledTimes(1)
  })

  /// A "delete" that keeps every file is only reassuring if it says so.
  it("says where the files go", async () => {
    const user = userEvent.setup()
    render(
      <DeleteConfirm
        busy={false}
        label="Delete session Standup"
        onDelete={vi.fn()}
        what="session"
      />,
    )

    await user.click(
      screen.getByRole("button", { name: "Delete session Standup" }),
    )

    expect(
      screen.getByText(/move this session to the workspace trash folder/i),
    ).toBeInTheDocument()
    expect(screen.getByText(/nothing is erased/i)).toBeInTheDocument()
  })

  it("backs out without removing anything", async () => {
    const user = userEvent.setup()
    const onDelete = vi.fn()
    render(
      <DeleteConfirm
        busy={false}
        label="Delete project Planning"
        onDelete={onDelete}
        what="project"
      />,
    )

    await user.click(
      screen.getByRole("button", { name: "Delete project Planning" }),
    )
    await user.click(screen.getByRole("button", { name: "Cancel" }))

    expect(onDelete).not.toHaveBeenCalled()
    expect(
      screen.getByRole("button", { name: "Delete project Planning" }),
    ).toBeInTheDocument()
  })
})
