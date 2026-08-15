import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { LibrarySnapshot } from "../types";
import { LibraryHealth } from "./LibraryHealth";

const baseSnapshot: LibrarySnapshot = {
  totalAssets: 12,
  duplicateAssets: 0,
  removedAssets: 0,
  missingAssets: 0,
  hashingAssets: false,
  packs: [],
  collections: [],
  projects: [],
  typeCounts: [],
};

const callbacks = {
  onViewMissing: vi.fn(),
  onViewRemoved: vi.fn(),
  onRelocatePack: vi.fn(),
  onRelocateProject: vi.fn(),
  onViewProject: vi.fn(),
  onRefreshProject: vi.fn(),
};

describe("LibraryHealth recovery", () => {
  it("offers direct recovery for disconnected packs and projects", async () => {
    const user = userEvent.setup();
    const snapshot: LibrarySnapshot = {
      ...baseSnapshot,
      packs: [{ id: 3, name: "Stone", rootPath: "/gone/stone", assetCount: 4, lastScannedAt: null, available: false, removedAssetCount: 0, missingAssetCount: 0 }],
      projects: [{ id: 7, name: "Harbor", rootPath: "/gone/harbor", assetCount: 2, available: false, lastExportedAt: null }],
    };
    render(<LibraryHealth snapshot={snapshot} {...callbacks} />);

    const packs = screen.getByRole("heading", { name: "Reconnect packs" }).parentElement!;
    const projects = screen.getByRole("heading", { name: "Reconnect projects" }).parentElement!;
    await user.click(within(packs).getByRole("button", { name: "Locate" }));
    await user.click(within(projects).getByRole("button", { name: "Locate" }));

    expect(callbacks.onRelocatePack).toHaveBeenCalledWith(3);
    expect(callbacks.onRelocateProject).toHaveBeenCalledWith(7);
  });

  it("treats recoverable removals as neutral history instead of an error", () => {
    render(<LibraryHealth snapshot={{ ...baseSnapshot, removedAssets: 4 }} {...callbacks} />);
    expect(screen.getByText("Clear")).toBeInTheDocument();
    expect(screen.getByText(/4 removals remain available to restore/)).toBeInTheDocument();
  });
});
