import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset } from "../types";
import { AssetCard } from "./AssetCard";

const playback = { path: null, playing: false };
vi.mock("../audioPlayback", () => ({
  subscribeAudioPlayback: () => () => {},
  getAudioPlaybackSnapshot: () => playback,
  toggleAudioPlayback: vi.fn(),
}));

vi.mock("./ModelCardPreview", () => ({
  ModelCardPreview: ({ asset }: { asset: Asset }) => <span data-testid="generated-model-preview" data-thumbnail={asset.thumbnailPath ?? "none"} />,
}));

const asset: Asset = {
  id: 1, packId: 1, packName: "Pack", name: "tone", relativePath: "Audio/tone.wav",
  absolutePath: "/tmp/tone.wav", extension: "wav", assetType: "audio", fileType: "audio",
  usage: null, mapRole: null, resolution: null, classificationConfidence: 100,
  classificationBasis: "extension", sizeBytes: 1024, modifiedAt: 1, width: null, height: null,
  thumbnailPath: null, variants: [], resources: [], tags: [], collectionIds: [], missing: false,
  manualClassification: false, contentHash: null, duplicateCount: 0, duplicateLocations: [],
};

const callbacks = {
  onSelect: vi.fn(), onContextSelect: vi.fn(), onOpen: vi.fn(), onReveal: vi.fn(),
  onRemove: vi.fn(), onRestore: vi.fn(), onCopyPath: vi.fn(), onError: vi.fn(), onPreviewError: vi.fn(),
};
const optionProps = { optionId: "asset-option-1", optionIndex: 0, optionCount: 5, tabIndex: 0 };

describe("AssetCard compact previews", () => {
  beforeEach(() => vi.clearAllMocks());

  it("centers the audio control over the list thumbnail", () => {
    render(<AssetCard asset={asset} selected={false} view="list" removed={false} selectionCount={1} dragPaths={[]} {...callbacks} {...optionProps} />);
    expect(screen.getByRole("button", { name: "Play tone" })).toHaveClass("top-1/2", "left-[25px]", "-translate-x-1/2", "-translate-y-1/2");
  });

  it("regenerates a model preview when its saved thumbnail cannot load", async () => {
    const model = { ...asset, name: "crate", assetType: "model" as const, fileType: "model" as const, extension: "glb", thumbnailPath: "/missing/crate.png" };
    const { container } = render(<AssetCard asset={model} selected={false} view="grid" removed={false} selectionCount={1} dragPaths={[]} {...callbacks} {...optionProps} />);
    fireEvent.error(container.querySelector("img")!);
    expect(await screen.findByTestId("generated-model-preview")).toHaveAttribute("data-thumbnail", "none");
  });

  it("exposes roving listbox option semantics", () => {
    render(<AssetCard asset={asset} selected view="list" removed={false} selectionCount={1} dragPaths={[]} {...callbacks} {...optionProps} />);
    const option = screen.getByRole("option", { name: /tone/i });
    expect(option).toHaveAttribute("aria-selected", "true");
    expect(option).toHaveAttribute("aria-posinset", "1");
    expect(option).toHaveAttribute("aria-setsize", "5");
    expect(option).toHaveAttribute("tabindex", "0");
  });

  it("offers project removal instead of Lootbox removal in a project view", async () => {
    render(<AssetCard asset={asset} selected view="grid" removed={false} projectAsset selectionCount={1} dragPaths={[]} {...callbacks} {...optionProps} />);
    fireEvent.contextMenu(screen.getByRole("option", { name: /tone/i }));
    expect(await screen.findByText("Remove from project")).toBeInTheDocument();
    expect(screen.queryByText("Remove from Lootbox")).not.toBeInTheDocument();
  });

  it("keeps thumbnail failures local and offers retry", () => {
    const image = { ...asset, name: "wall", assetType: "image" as const, fileType: "image" as const, extension: "png" };
    const { container } = render(<AssetCard asset={image} selected={false} view="grid" removed={false} selectionCount={1} dragPaths={[]} {...callbacks} {...optionProps} />);
    fireEvent.error(container.querySelector("img")!);
    expect(callbacks.onError).not.toHaveBeenCalled();
    expect(callbacks.onPreviewError).toHaveBeenCalledWith(image, expect.any(Error));
    fireEvent.click(screen.getByRole("button", { name: "Retry preview for wall" }));
    expect(screen.queryByRole("button", { name: "Retry preview for wall" })).not.toBeInTheDocument();
  });
});
