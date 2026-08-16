import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import type { LibrarySnapshot } from "../types";
import { CommandPalette, type CommandPaletteProps } from "./CommandPalette";

beforeAll(() => {
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
});

const mockSnapshot: LibrarySnapshot = {
  totalAssets: 150,
  duplicateAssets: 4,
  removedAssets: 1,
  missingAssets: 0,
  hashingAssets: false,
  packs: [
    {
      id: 1,
      name: "Kenney SciFi Kit",
      rootPath: "/assets/scifi",
      assetCount: 80,
      lastScannedAt: null,
      available: true,
      removedAssetCount: 0,
      missingAssetCount: 0,
    },
    {
      id: 2,
      name: "Medieval Audio",
      rootPath: "/assets/medieval-audio",
      assetCount: 70,
      lastScannedAt: null,
      available: true,
      removedAssetCount: 1,
      missingAssetCount: 0,
    },
  ],
  collections: [
    { id: 10, name: "Boss Fight Assets", assetCount: 12 },
  ],
  projects: [
    {
      id: 100,
      name: "Space Arcade",
      rootPath: "/games/space-arcade",
      assetCount: 25,
      available: true,
      lastExportedAt: null,
    },
  ],
  typeCounts: [
    { assetType: "model", count: 80 },
    { assetType: "audio", count: 70 },
  ],
};

const defaultProps: CommandPaletteProps = {
  open: true,
  onOpenChange: vi.fn(),
  snapshot: mockSnapshot,
  activeProject: mockSnapshot.projects[0],
  savedViews: [
    {
      id: "view-1",
      name: "Unused 3D Models",
      query: "model",
      filters: { extension: "", mapRole: "", tag: "", minWidth: "", minConfidence: "", status: "", projectUsage: "" },
      selection: { kind: "all" },
      sort: "name",
      sortDirection: "asc",
    },
  ],
  selectedCount: 3,
  view: "grid",
  leftPanelCollapsed: false,
  rightPanelCollapsed: false,
  onSelectScope: vi.fn(),
  onOpenSavedView: vi.fn(),
  onActivateProject: vi.fn(),
  onImportPack: vi.fn(),
  onStartCollection: vi.fn(),
  onSaveCurrentView: vi.fn(),
  onAddProject: vi.fn(),
  onExportToActiveProject: vi.fn(),
  onOpenSettings: vi.fn(),
  onOpenShortcuts: vi.fn(),
  onSetView: vi.fn(),
  onToggleSidebar: vi.fn(),
  onToggleDetailPanel: vi.fn(),
  onSelectAll: vi.fn(),
  onClearSelection: vi.fn(),
  onSetFilterType: vi.fn(),
  onSetSort: vi.fn(),
  onCleanCache: vi.fn(),
  onClearCache: vi.fn(),
};

describe("CommandPalette component", () => {
  it("renders the palette with search input and categorized suggestions when open", () => {
    render(<CommandPalette {...defaultProps} />);

    const searchInput = screen.getByPlaceholderText(/Type a command or search/i);
    expect(searchInput).toBeInTheDocument();
    expect(screen.getByText("Kenney SciFi Kit")).toBeInTheDocument();
    expect(screen.getAllByText("Space Arcade").length).toBeGreaterThan(0);
    expect(screen.getByText("Boss Fight Assets")).toBeInTheDocument();
    expect(screen.getByText("Unused 3D Models")).toBeInTheDocument();
  });

  it("filters items dynamically when typing a search query", async () => {
    const user = userEvent.setup();
    render(<CommandPalette {...defaultProps} />);

    const searchInput = screen.getByPlaceholderText(/Type a command or search/i);
    await user.type(searchInput, "audio");

    expect(screen.getByText("Medieval Audio")).toBeInTheDocument();
    expect(screen.queryByText("Kenney SciFi Kit")).not.toBeInTheDocument();
  });

  it("executes the selected action and closes palette on click", async () => {
    const user = userEvent.setup();
    const onSelectScope = vi.fn();
    const onOpenChange = vi.fn();

    render(
      <CommandPalette
        {...defaultProps}
        onSelectScope={onSelectScope}
        onOpenChange={onOpenChange}
      />,
    );

    const packOption = screen.getByText("Kenney SciFi Kit");
    await user.click(packOption);

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onSelectScope).toHaveBeenCalledWith({ kind: "pack", packId: 1 });
  });

  it("navigates with keyboard and executes with Enter", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    const onExportToActiveProject = vi.fn();

    render(
      <CommandPalette
        {...defaultProps}
        selectedCount={3}
        onExportToActiveProject={onExportToActiveProject}
        onOpenChange={onOpenChange}
      />,
    );

    const searchInput = screen.getByPlaceholderText(/Type a command or search/i);
    await user.type(searchInput, "Export");

    await user.keyboard("{Enter}");

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onExportToActiveProject).toHaveBeenCalled();
  });
});
