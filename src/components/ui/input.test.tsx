import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Input } from "./input";

describe("Input context menu", () => {
  it("uses the Lootbox editing menu", async () => {
    render(<Input aria-label="Name" defaultValue="ambience" />);
    fireEvent.contextMenu(screen.getByLabelText("Name"));
    expect(await screen.findByRole("menuitem", { name: /Undo/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /Select all/ })).toBeInTheDocument();
  });
});
