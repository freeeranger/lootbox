import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { Asset } from "../types";
import { DetailPanel } from "./DetailPanel";

const asset: Asset = {
  id: 1,
  packId: 1,
  packName: "Test pack",
  name: "wall",
  relativePath: "Textures/wall.png",
  absolutePath: "/tmp/wall.png",
  extension: "png",
  assetType: "image",
  fileType: "image",
  usage: null,
  mapRole: null,
  resolution: null,
  classificationConfidence: 55,
  classificationBasis: "map-role-filename",
  sizeBytes: 1024,
  modifiedAt: 1,
  width: 512,
  height: 512,
  thumbnailPath: null,
  variants: [],
  resources: [],
  tags: [],
  collectionIds: [],
  missing: true,
  manualClassification: false,
  contentHash: null,
  duplicateCount: 0,
  duplicateLocations: [],
};

function renderPanel(overrides: Partial<React.ComponentProps<typeof DetailPanel>> = {}) {
  const props: React.ComponentProps<typeof DetailPanel> = {
    asset,
    selectedCount: 3,
    selectedAssets: [asset, { ...asset, id: 2 }, { ...asset, id: 3 }],
    collections: [],
    onAddTag: vi.fn(),
    onRemoveTag: vi.fn(),
    onMembership: vi.fn(),
    onOpen: vi.fn(),
    onOpenVariant: vi.fn(),
    onCopyPath: vi.fn(),
    onRevealPath: vi.fn(),
    onClassification: vi.fn(),
    onGroup: vi.fn(),
    onResetClassification: vi.fn(),
    onPreviewError: vi.fn(),
    onAddCollection: vi.fn(),
    ...overrides,
  };
  render(<TooltipProvider><DetailPanel {...props} /></TooltipProvider>);
  return props;
}

describe("DetailPanel classification controls", () => {
  it("applies a type correction to the selected assets", async () => {
    const user = userEvent.setup();
    const props = renderPanel();
    await user.click(screen.getByLabelText("Asset type"));
    await user.click(await screen.findByRole("option", { name: "Texture" }));
    expect(props.onClassification).toHaveBeenCalledWith("texture");
    expect(screen.getByText("Classification & grouping")).toBeInTheDocument();
    expect(screen.getByText("3 selected")).toBeInTheDocument();
  });

  it("offers persistent grouping corrections and identifies missing files", () => {
    const props = renderPanel();
    fireEvent.click(screen.getByRole("button", { name: "Group selected" }));
    expect(props.onGroup).toHaveBeenCalledWith("merge");
    expect(screen.getByText("Missing from disk")).toBeInTheDocument();
    expect(screen.getByText(/survive rescans/)).toBeInTheDocument();
  });

  it("shows mixed bulk values instead of the active asset value", () => {
    renderPanel({
      selectedCount: 2,
      selectedAssets: [asset, { ...asset, id: 2, assetType: "texture", mapRole: "normal" }],
    });
    expect(screen.getByLabelText("Asset type")).toHaveTextContent("Multiple values");
    expect(screen.getByLabelText("Texture map role")).toHaveTextContent("Multiple values");
  });

  it("can explicitly clear a texture map role", async () => {
    const user = userEvent.setup();
    const texture = { ...asset, assetType: "texture" as const, usage: "texture" as const, mapRole: "color" };
    const props = renderPanel({ asset: texture, selectedCount: 1, selectedAssets: [texture] });
    await user.click(screen.getByLabelText("Texture map role"));
    await user.click(await screen.findByRole("option", { name: "No map role" }));
    expect(props.onClassification).toHaveBeenCalledWith(undefined, "__none");
  });

  it("previews a grouped resource on one click and opens it on double click", async () => {
    const user = userEvent.setup();
    const resource = {
      id: 9,
      name: "wall_normal",
      extension: "png",
      assetType: "texture" as const,
      fileType: "image" as const,
      usage: "texture" as const,
      mapRole: "normal",
      resolution: "512",
      absolutePath: "/tmp/wall_normal.png",
      relativePath: "Textures/wall_normal.png",
      sizeBytes: 2048,
      thumbnailPath: null,
    };
    const grouped = { ...asset, resources: [resource] };
    const props = renderPanel({ asset: grouped, selectedCount: 1, selectedAssets: [grouped] });
    const resourceButton = screen.getByRole("button", { name: "wall_normal" });
    await user.click(resourceButton);
    expect(props.onOpenVariant).not.toHaveBeenCalled();
    expect(screen.getByText("Previewing wall_normal · .png")).toBeInTheDocument();
    await user.dblClick(resourceButton);
    expect(props.onOpenVariant).toHaveBeenCalledWith("/tmp/wall_normal.png");
  });

  it("shows mixed collection membership as indeterminate", () => {
    const first = { ...asset, collectionIds: [4] };
    const second = { ...asset, id: 2, collectionIds: [] };
    renderPanel({
      asset: first,
      selectedCount: 2,
      selectedAssets: [first, second],
      collections: [{ id: 4, name: "Environment", assetCount: 1 }],
    });
    expect(screen.getByRole("checkbox", { name: "Environment" })).toHaveAttribute("data-indeterminate");
  });
});
