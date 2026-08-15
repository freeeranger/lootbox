import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { LibrarySelection, LibrarySnapshot } from "../types";
import { Sidebar } from "./Sidebar";

const baseSnapshot: LibrarySnapshot = {
  totalAssets: 42,
  duplicateAssets: 2,
  removedAssets: 0,
  missingAssets: 0,
  hashingAssets: false,
  packs: [
    { id: 1, name: "Castle Kit", rootPath: "/assets/castle", assetCount: 20, lastScannedAt: null, available: true, removedAssetCount: 0, missingAssetCount: 0 },
  ],
  collections: [],
  projects: [
    { id: 10, name: "Space Shooter", rootPath: "/games/space-shooter", assetCount: 15, available: true, lastExportedAt: null },
  ],
  typeCounts: [{ assetType: "model", count: 20 }, { assetType: "texture", count: 22 }],
};

const defaultProps = {
  snapshot: baseSnapshot,
  selection: { kind: "all" } as LibrarySelection,
  creatingCollection: false,
  activeProjectId: null,
  activeProjectAttention: 0,
  savedViews: [],
  activeSavedViewId: null,
  onSelect: vi.fn(),
  onActivateProject: vi.fn(),
  onRelocateProject: vi.fn(),
  onOpenSavedView: vi.fn(),
  onDeleteSavedView: vi.fn(),
  onImport: vi.fn(),
  onStartCollection: vi.fn(),
  onRenamePack: vi.fn(),
  onRescanPack: vi.fn(),
  onOpenPack: vi.fn(),
  onRelocatePack: vi.fn(),
  onForgetPack: vi.fn(),
  onViewRemoved: vi.fn(),
  onViewMissing: vi.fn(),
  onPurgeMissing: vi.fn(),
  onAddProject: vi.fn(),
  onOpenProject: vi.fn(),
  onForgetProject: vi.fn(),
  onSettings: vi.fn(),
  onShortcuts: vi.fn(),
};

describe("Sidebar component", () => {
  it("renders global library mode and toggles workspace menu cleanly", async () => {
    render(<Sidebar {...defaultProps} />);

    expect(screen.getByText("Global Library")).toBeInTheDocument();
    expect(screen.getByText("42 assets")).toBeInTheDocument();

    const trigger = screen.getByRole("button", { name: /Global Library/i });
    trigger.click();

    expect(screen.getByText("Global Library")).toBeInTheDocument();
  });

  it("renders active project mode with project navigation", () => {
    render(
      <Sidebar
        {...defaultProps}
        activeProjectId={10}
        selection={{ kind: "project", projectId: 10 }}
      />,
    );

    expect(screen.getByText("Space Shooter")).toBeInTheDocument();
    expect(screen.getByText("Project assets")).toBeInTheDocument();
    expect(screen.getByText("Project sync & health")).toBeInTheDocument();
  });
});
