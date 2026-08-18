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

  it("naturally sorts packs, collections, and projects taking full numbers into account", () => {
    const unsortedSnapshot: LibrarySnapshot = {
      ...baseSnapshot,
      packs: [
        { id: 1, name: "pack vol 56", rootPath: "/assets/p56", assetCount: 5, lastScannedAt: null, available: true, removedAssetCount: 0, missingAssetCount: 0 },
        { id: 2, name: "pack vol 9", rootPath: "/assets/p9", assetCount: 5, lastScannedAt: null, available: true, removedAssetCount: 0, missingAssetCount: 0 },
        { id: 3, name: "pack vol 2", rootPath: "/assets/p2", assetCount: 5, lastScannedAt: null, available: true, removedAssetCount: 0, missingAssetCount: 0 },
      ],
      collections: [
        { id: 1, name: "Hero 100", assetCount: 3 },
        { id: 2, name: "Hero 9", assetCount: 3 },
        { id: 3, name: "Hero 20", assetCount: 3 },
      ],
    };

    render(<Sidebar {...defaultProps} snapshot={unsortedSnapshot} />);

    const packElements = screen.getAllByRole("button", { name: /pack vol/i });
    expect(packElements.map((el) => el.textContent)).toEqual([
      expect.stringContaining("pack vol 2"),
      expect.stringContaining("pack vol 9"),
      expect.stringContaining("pack vol 56"),
    ]);

    const collectionElements = screen.getAllByRole("button", { name: /Hero/i });
    expect(collectionElements.map((el) => el.textContent)).toEqual([
      expect.stringContaining("Hero 9"),
      expect.stringContaining("Hero 20"),
      expect.stringContaining("Hero 100"),
    ]);
  });
});

