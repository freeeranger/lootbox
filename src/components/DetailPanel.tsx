import { lazy, Suspense, useEffect, useState } from "react";
import type { RefObject } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Clipboard, ExternalLink, FolderOpen, Plus, RotateCcw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { cn, collapseHomePath } from "@/lib/utils";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { Asset, AssetResource, AssetType, CollectionSummary, ModelStats } from "../types";
import { AssetTypeIcon } from "./AssetTypeIcon";
import { AudioPreview } from "./AudioPreview";

const ModelPreview = lazy(() =>
  import("./ModelPreview").then((module) => ({ default: module.ModelPreview })),
);

const browserImages = new Set(["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"]);

interface Props {
  asset: Asset;
  selectedCount: number;
  selectedAssets: Asset[];
  tagInputRef: RefObject<HTMLInputElement | null>;
  busy: boolean;
  collections: CollectionSummary[];
  onAddTag: (name: string) => Promise<void>;
  onRemoveTag: (name: string) => Promise<void>;
  onMembership: (collectionId: number, included: boolean) => Promise<void>;
  onOpen: () => void;
  onOpenVariant: (path: string) => void;
  onCopyPath: (path: string) => void;
  onRevealPath: (path: string) => void;
  onClassification: (assetType?: string, mapRole?: string) => Promise<void>;
  onGroup: (action: "merge" | "split") => Promise<void>;
  onResetClassification: () => Promise<void>;
  onAddCollection: () => void;
}

type PreviewAsset = Pick<Asset, "id" | "name" | "absolutePath" | "extension" | "assetType">;

const assetTypeNames: Record<AssetType, string> = {
  image: "Image",
  texture: "Texture",
  audio: "Audio",
  model: "Model",
  video: "Video",
  font: "Font",
  shader: "Shader",
  material: "Material",
  archive: "Archive",
  other: "Other",
};

function fileName(relativePath: string) {
  return relativePath.split("/").pop()?.replace(/\.[^.]+$/, "") ?? relativePath;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index++) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${unit}`;
}

function supportCopyLabel(relativePath: string) {
  const format = relativePath.match(/\/(glb|gltf|fbx|obj|dae|blend)\//i)?.[1];
  return format ? `${format.toUpperCase()} copy` : "Model copy";
}

function readableMapRole(role: string) {
  return role.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

function textureMapLabel(resource: AssetResource) {
  const relativePath = resource.relativePath;
  const parts = relativePath.split(/[\\/]/);
  const resolution = parts.find((part) => /^(?:\d{2,5}(?:x\d{2,5})?|\d{1,2}k)$/i.test(part));
  const directories = parts.slice(0, -1).map((part) =>
    part.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, ""),
  );
  const stem = (parts.at(-1) ?? "").replace(/\.[^.]+$/, "").toLowerCase();
  const hasDirectory = (...names: string[]) => directories.some((part) => names.includes(part));
  const hasSuffix = (...names: string[]) => names.some((name) => stem.endsWith(`_${name}`));
  const role = hasDirectory("normal", "normals", "normal_map", "normal_maps") || hasSuffix("normal", "nrm")
    ? "Normal"
    : hasDirectory("rough", "roughness", "roughness_map", "roughness_maps") || hasSuffix("rough", "roughness")
      ? "Roughness"
      : hasDirectory("metallic", "metalness", "metallic_maps", "metalness_maps") || hasSuffix("metallic", "metalness")
        ? "Metalness"
        : hasDirectory("ao", "occlusion", "ambient_occlusion", "occlusion_maps") || hasSuffix("ao", "occlusion")
          ? "Occlusion"
          : hasDirectory("height", "displacement", "height_maps", "displacement_maps") || hasSuffix("height", "displacement", "disp", "bump")
            ? "Height"
            : hasDirectory("emission", "emissive", "emission_maps", "emissive_maps") || hasSuffix("emission", "emissive")
              ? "Emission"
              : hasDirectory("opacity", "alpha", "opacity_maps", "alpha_maps") || hasSuffix("opacity", "alpha")
                ? "Opacity"
                : "Color";
  const detectedRole = resource.mapRole ? readableMapRole(resource.mapRole) : role;
  const detectedResolution = resource.resolution ?? resolution;
  return detectedResolution ? `${detectedResolution} · ${detectedRole}` : detectedRole;
}

function textureResourceRank(resource: AssetResource) {
  const roleOrder = [
    "color", "normal", "normal_gl", "normal_dx", "roughness", "metalness",
    "occlusion", "occlusion_roughness_metalness", "roughness_metalness_occlusion",
    "height", "opacity", "emissive", "specular", "glossiness",
  ];
  const role = roleOrder.indexOf(resource.mapRole ?? "");
  const resolution = Number.parseInt(resource.resolution ?? "0", 10) || 0;
  return [(role < 0 ? roleOrder.length : role), -resolution] as const;
}

function Preview({ asset, onModelStats }: { asset: PreviewAsset; onModelStats: (stats: ModelStats) => void }) {
  const frame = "mx-4 h-[232px] overflow-hidden rounded-md border bg-muted/10";
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => setFailed(false), [asset.absolutePath]);

  if (failed) {
    return (
      <div className={`${frame} grid place-items-center px-6 text-center`} role="status">
        <div>
          <AssetTypeIcon type={asset.assetType} size={32} strokeWidth={1.25} />
          <p className="mt-2 text-xs text-muted-foreground">Preview unavailable for this file.</p>
          <Button type="button" variant="outline" size="xs" className="mt-3 rounded-sm" onClick={() => { setFailed(false); setAttempt((current) => current + 1); }}>
            <RotateCcw /> Retry preview
          </Button>
        </div>
      </div>
    );
  }

  if (["image", "texture"].includes(asset.assetType) && browserImages.has(asset.extension)) {
    return (
      <div className={`${frame} checkerboard p-2`}>
        <div className="relative size-full min-h-0 min-w-0">
          <img
            key={attempt}
            src={convertFileSrc(asset.absolutePath)}
            alt={asset.name}
            className="absolute inset-0 size-full object-contain"
            onError={() => setFailed(true)}
          />
        </div>
      </div>
    );
  }
  if (asset.assetType === "audio") return <AudioPreview path={asset.absolutePath} />;
  if (asset.assetType === "video") {
    return (
      <div className={frame}>
        <video
          key={attempt}
          src={convertFileSrc(asset.absolutePath)}
          controls
          preload="metadata"
          className="size-full object-contain"
          onError={() => setFailed(true)}
        />
      </div>
    );
  }
  if (asset.assetType === "model" && ["glb", "gltf"].includes(asset.extension)) {
    return (
      <Suspense
        fallback={
          <div className={`${frame} grid place-items-center text-xs text-muted-foreground`}>
            Loading…
          </div>
        }
      >
      <ModelPreview path={asset.absolutePath} onStats={onModelStats} />
      </Suspense>
    );
  }
  return (
    <div className={`${frame} grid place-items-center`}>
      <div className="text-center text-muted-foreground">
        <AssetTypeIcon type={asset.assetType} size={36} strokeWidth={1.25} />
        <span className="mt-2 block font-mono text-[11px]">
          {asset.extension ? `.${asset.extension}` : asset.assetType}
        </span>
      </div>
    </div>
  );
}

function IconAction({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="rounded-sm text-muted-foreground"
            onClick={onClick}
            aria-label={label}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function DetailPanel({
  asset,
  selectedCount,
  selectedAssets,
  tagInputRef,
  busy,
  collections,
  onAddTag,
  onRemoveTag,
  onMembership,
  onOpen,
  onOpenVariant,
  onCopyPath,
  onRevealPath,
  onClassification,
  onGroup,
  onResetClassification,
  onAddCollection,
}: Props) {
  const [tag, setTag] = useState("");
  const [modelStats, setModelStats] = useState<ModelStats | null>(null);
  const [previewAsset, setPreviewAsset] = useState<PreviewAsset>(asset);
  const allSelectedKnown = selectedAssets.length === selectedCount;
  const selectionValue = <T,>(read: (item: Asset) => T): T | "__mixed" => {
    if (selectedCount === 1) return read(asset);
    if (!allSelectedKnown || selectedAssets.length === 0) return "__mixed";
    const values = new Set(selectedAssets.map(read));
    return values.size === 1 ? read(selectedAssets[0]!) : "__mixed";
  };
  const selectedType = selectionValue((item) => item.assetType);
  const selectedMapRole = selectionValue((item) => item.mapRole ?? "__none");
  const canGroupSelection = selectedCount > 1 && allSelectedKnown &&
    new Set(selectedAssets.map((item) => item.packId)).size === 1;
  const visibleTags = selectedCount === 1
    ? asset.tags.map((name) => ({ name, partial: false }))
    : allSelectedKnown
      ? [...new Set(selectedAssets.flatMap((item) => item.tags))].map((name) => ({
          name,
          partial: !selectedAssets.every((item) => item.tags.includes(name)),
        }))
      : [];

  useEffect(() => {
    setModelStats(null);
    setPreviewAsset(asset);
  }, [asset.id]);

  useEffect(() => setModelStats(null), [previewAsset.absolutePath]);

  async function submitTag(event: React.FormEvent) {
    event.preventDefault();
    const value = tag.trim();
    if (!value) return;
    await onAddTag(value);
    setTag("");
  }

  return (
    <aside className="quiet-scrollbar h-full min-w-0 overflow-y-auto border-l bg-background">
      <header className="sticky top-0 z-20 flex h-[58px] items-center gap-2 border-b bg-background/95 px-3 backdrop-blur-sm">
        <div className="min-w-0 flex-1">
          {selectedCount > 1 ? (
            <div>
              <div className="flex min-w-0 items-center gap-1.5">
                <span className="inline-flex items-center gap-1 rounded-xs border border-primary/40 bg-primary/15 px-1.5 py-0.5 font-mono text-[11px] font-semibold text-primary">
                  {selectedCount.toLocaleString()} selected
                </span>
                <h2 className="truncate text-xs font-semibold text-foreground">Batch inspection</h2>
              </div>
              <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                Editing tags, collections & categories across selection
              </p>
            </div>
          ) : (
            <div>
              <h2 className="truncate text-xs font-semibold">{asset.name}</h2>
              <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                {previewAsset.absolutePath === asset.absolutePath
                  ? `${asset.packName} · ${assetTypeNames[asset.assetType]}${asset.resolution ? ` · ${asset.resolution}` : ""}`
                  : `Previewing ${previewAsset.name} · .${previewAsset.extension}`}
              </p>
            </div>
          )}
        </div>
        {selectedCount === 1 && <div className="flex items-center">
          <IconAction label="Open previewed file" onClick={() => previewAsset.absolutePath === asset.absolutePath ? onOpen() : onOpenVariant(previewAsset.absolutePath)}>
            <ExternalLink />
          </IconAction>
          <IconAction label="Reveal in folder" onClick={() => onRevealPath(previewAsset.absolutePath)}>
            <FolderOpen />
          </IconAction>
          <IconAction
            label="Copy path"
            onClick={() => onCopyPath(previewAsset.absolutePath)}
          >
            <Clipboard />
          </IconAction>
        </div>}
      </header>

      {selectedCount === 1 && <div className="pt-4">
        <Preview asset={previewAsset} onModelStats={setModelStats} />
      </div>}

      <div className="space-y-5 px-4 py-5 [&>section>h3]:text-[11px] [&>section>h3]:font-medium [&>section>h3]:text-foreground">
        {busy && <p className="rounded-sm border bg-muted/20 px-2 py-1.5 text-[11px] text-muted-foreground" role="status" aria-live="polite">Saving changes to {selectedCount.toLocaleString()} {selectedCount === 1 ? "asset" : "assets"}…</p>}
        {selectedCount === 1 ? <section>
          <h3 className="mb-2 text-[11px] font-medium">Info</h3>
          <dl className="grid grid-cols-[76px_minmax(0,1fr)] gap-y-1.5 text-[11px]">
            <dt className="text-muted-foreground">Category</dt>
            <dd>{assetTypeNames[asset.assetType]}</dd>
            {asset.missing && (
              <>
                <dt className="text-muted-foreground">Status</dt>
                <dd className="text-destructive">Missing from disk</dd>
              </>
            )}
            {asset.usage && (
              <>
                <dt className="text-muted-foreground">Source type</dt>
                <dd>{assetTypeNames[asset.fileType]}</dd>
              </>
            )}
            {asset.mapRole && (
              <>
                <dt className="text-muted-foreground">Map</dt>
                <dd>{readableMapRole(asset.mapRole)}</dd>
              </>
            )}
            {asset.resolution && (
              <>
                <dt className="text-muted-foreground">Resolution</dt>
                <dd>{asset.resolution}</dd>
              </>
            )}
            <dt className="text-muted-foreground">Format</dt>
            <dd>{asset.extension ? `.${asset.extension}` : "—"}</dd>
            <dt className="text-muted-foreground">Size</dt>
            <dd>{formatBytes(asset.sizeBytes)}</dd>
            {asset.width && asset.height && (
              <>
                <dt className="text-muted-foreground">Dimensions</dt>
                <dd>{asset.width} × {asset.height}</dd>
              </>
            )}
            {asset.assetType === "model" && modelStats && (
              <>
                <dt className="text-muted-foreground">Triangles</dt>
                <dd>{modelStats.triangles.toLocaleString()}</dd>
                <dt className="text-muted-foreground">Vertices</dt>
                <dd>{modelStats.vertices.toLocaleString()}</dd>
              </>
            )}
            <dt className="text-muted-foreground">Path</dt>
            <dd className="truncate" title={asset.relativePath}>{asset.relativePath}</dd>
            {asset.usage && (
              <>
                <dt className="text-muted-foreground">Detected</dt>
                <dd
                  className="truncate"
                  title={asset.classificationBasis.replaceAll(",", ", ")}
                >
                  {asset.classificationConfidence}% confidence
                </dd>
              </>
            )}
          </dl>

          {asset.variants.length > 1 && asset.assetType !== "texture" && (
            <div className="mt-3">
              <h3 className="mb-1.5 text-[11px] font-medium">
                {asset.assetType === "model" ? "Formats" : "Copies"}
              </h3>
              <div className="flex flex-wrap gap-1">
                {[...asset.variants]
                  .sort((left, right) => {
                    if (left.id === asset.id) return -1;
                    if (right.id === asset.id) return 1;
                    const order = ["glb", "gltf", "fbx", "obj", "dae", "blend", "mtl"];
                    const leftRank = order.indexOf(left.extension);
                    const rightRank = order.indexOf(right.extension);
                    return (leftRank < 0 ? order.length : leftRank) -
                      (rightRank < 0 ? order.length : rightRank);
                  })
                  .map((variant) => (
                    <Button
                      type="button"
                      key={variant.id}
                      variant={variant.absolutePath === previewAsset.absolutePath ? "secondary" : "outline"}
                      size="xs"
                      className="h-7 rounded-sm px-2 font-mono text-[11px] font-normal uppercase"
                      onClick={() => setPreviewAsset({
                        id: variant.id,
                        name: fileName(variant.relativePath),
                        absolutePath: variant.absolutePath,
                        extension: variant.extension,
                        assetType: variant.assetType,
                      })}
                      onDoubleClick={() => onOpenVariant(variant.absolutePath)}
                      aria-pressed={variant.absolutePath === previewAsset.absolutePath}
                      title={`${variant.relativePath} · ${formatBytes(variant.sizeBytes)}`}
                    >
                      {asset.assetType === "model"
                        ? variant.extension
                        : variant.assetType === "texture"
                          ? "Texture"
                          : supportCopyLabel(variant.relativePath)}
                    </Button>
                  ))}
              </div>
            </div>
          )}

          {asset.duplicateLocations.length > 0 && (
            <div className="mt-3">
              <h3 className="mb-1.5 text-[11px] font-medium">
                Identical copies · {asset.duplicateCount}
              </h3>
              <div className="space-y-1">
                {asset.duplicateLocations.map((duplicate) => (
                  <button
                    type="button"
                    key={duplicate.id}
                    className="block w-full min-w-0 rounded-sm border px-2 py-1.5 text-left hover:bg-muted/40"
                    onClick={() => onOpenVariant(duplicate.absolutePath)}
                    title={collapseHomePath(duplicate.absolutePath)}
                  >
                    <span className="block truncate text-[11px] font-medium">{duplicate.packName}</span>
                    <span className="block truncate text-[11px] text-muted-foreground">
                      {duplicate.relativePath} · {formatBytes(duplicate.sizeBytes)}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </section> : <section className="rounded-md border bg-muted/10 p-3">
          <h3 className="mb-1.5">Bulk selection</h3>
          <p className="text-xs">Editing all {selectedCount.toLocaleString()} selected assets</p>
          <p className="mt-1 text-[11px] text-muted-foreground">
            {allSelectedKnown
              ? `${new Set(selectedAssets.map((item) => item.assetType)).size} asset types · ${new Set(selectedAssets.map((item) => item.packId)).size} source packs · mixed values are labeled below`
              : "The selection includes unloaded results; review the complete list before changing shared metadata."}
          </p>
          {selectedAssets.some((item) => item.missing) && <p className="mt-1 text-[11px] text-destructive">{selectedAssets.filter((item) => item.missing).length.toLocaleString()} missing from disk</p>}
        </section>}

        <Separator />

        <section>
          <h3 className="mb-2 flex items-center justify-between gap-2">
            <span>Classification & grouping</span><span className="font-normal text-muted-foreground">{selectedCount.toLocaleString()} {selectedCount === 1 ? "asset" : "assets"}</span>
          </h3>
          <div className="grid grid-cols-2 gap-1.5">
            <div>
              <span className="mb-1 block text-[11px] text-muted-foreground">Asset type</span>
            <Select
              items={[
                ...(selectedType === "__mixed" ? [{ value: "__mixed", label: "Multiple values" }] : []),
                ...Object.entries(assetTypeNames).map(([value, label]) => ({ value, label })),
              ]}
              value={selectedType}
              disabled={busy}
              onValueChange={(value) => { if (value && value !== "__mixed") void onClassification(value); }}
            >
              <SelectTrigger size="sm" aria-label="Asset type" className="w-full rounded-sm text-[11px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent alignItemWithTrigger={false} align="start">
                <SelectGroup>
                  {selectedType === "__mixed" && <SelectItem value="__mixed" disabled className="text-[11px]">Multiple values</SelectItem>}
                  {(Object.entries(assetTypeNames) as Array<[AssetType, string]>).map(([type, label]) => (
                    <SelectItem key={type} value={type} className="text-[11px]">{label}</SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            </div>
            <div>
              <span className="mb-1 block text-[11px] text-muted-foreground">Texture map</span>
            <Select
              items={[
                ...(selectedMapRole === "__mixed" ? [{ value: "__mixed", label: "Multiple values" }] : []),
                { value: "__none", label: "No map role" },
                ...["color", "normal", "normal_gl", "normal_dx", "roughness", "metalness", "occlusion", "height", "opacity", "emissive", "specular", "glossiness"].map((value) => ({ value, label: readableMapRole(value) })),
              ]}
              value={selectedMapRole}
              disabled={busy || selectedType !== "texture"}
              onValueChange={(value) => { if (value && value !== "__mixed") void onClassification(undefined, value); }}
            >
              <SelectTrigger size="sm" aria-label="Texture map role" className="w-full rounded-sm text-[11px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent alignItemWithTrigger={false} align="start">
                <SelectGroup>
                  {selectedMapRole === "__mixed" && <SelectItem value="__mixed" disabled className="text-[11px]">Multiple values</SelectItem>}
                  <SelectItem value="__none" className="text-[11px]">No map role</SelectItem>
                  {["color", "normal", "normal_gl", "normal_dx", "roughness", "metalness", "occlusion", "height", "opacity", "emissive", "specular", "glossiness"].map((role) => (
                    <SelectItem key={role} value={role} className="text-[11px]">{readableMapRole(role)}</SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            </div>
          </div>
          <div className="mt-1.5 flex flex-wrap gap-1.5">
            {selectedCount > 1 && (
              <Button type="button" variant="outline" size="xs" className="rounded-sm" disabled={busy || !canGroupSelection} title={canGroupSelection ? "Group selected assets" : "Assets from different packs cannot be grouped"} onClick={() => void onGroup("merge")}>Group selected</Button>
            )}
            {(selectedCount > 1 || asset.variants.length > 1 || asset.resources.length > 0) && (
              <Button type="button" variant="outline" size="xs" className="rounded-sm" disabled={busy} onClick={() => void onGroup("split")}>Remove from group</Button>
            )}
            {asset.manualClassification && (
              <Button type="button" variant="ghost" size="xs" className="rounded-sm" disabled={busy} onClick={() => void onResetClassification()}>Use automatic</Button>
            )}
          </div>
          <p className="mt-1.5 text-[11px] text-muted-foreground">
            Type controls how Lootbox previews a file. Texture map describes its material role; grouping keeps related files together while browsing and exporting. Manual choices survive rescans.
          </p>
        </section>

        {asset.resources.length > 0 && (
          <>
            <Separator />
            <section>
              <h3 className="mb-2 text-[11px] font-medium">
                {asset.assetType === "texture" ? "Maps & sizes" : "Textures"}
              </h3>
              <div className="grid grid-cols-2 gap-1.5">
                {[...asset.resources]
                  .sort((left, right) => {
                    const [leftRole, leftResolution] = textureResourceRank(left);
                    const [rightRole, rightResolution] = textureResourceRank(right);
                    return leftRole - rightRole || leftResolution - rightResolution ||
                      left.relativePath.localeCompare(right.relativePath);
                  })
                  .map((resource) => (
                  <button
                    type="button"
                    key={resource.id}
                    className={cn(
                      "group min-w-0 overflow-hidden rounded-sm border bg-muted/10 text-left hover:border-foreground/25",
                      resource.absolutePath === previewAsset.absolutePath && "border-primary ring-1 ring-primary/30",
                    )}
                    onClick={() => setPreviewAsset({
                      id: resource.id,
                      name: resource.name,
                      absolutePath: resource.absolutePath,
                      extension: resource.extension,
                      assetType: resource.assetType,
                    })}
                    onDoubleClick={() => onOpenVariant(resource.absolutePath)}
                    aria-pressed={resource.absolutePath === previewAsset.absolutePath}
                    title={resource.relativePath}
                  >
                    <span className="checkerboard relative block aspect-square overflow-hidden border-b">
                      {resource.thumbnailPath ? (
                        <img
                          src={convertFileSrc(resource.thumbnailPath)}
                          alt=""
                          className="absolute inset-0 size-full object-contain"
                        />
                      ) : (
                        <span className="grid size-full place-items-center text-muted-foreground">
                          <AssetTypeIcon type={resource.assetType} size={18} />
                        </span>
                      )}
                    </span>
                    <span className="block truncate px-1.5 py-1 text-[11px]">
                      {asset.assetType === "texture"
                        ? textureMapLabel(resource)
                        : resource.name}
                    </span>
                  </button>
                  ))}
              </div>
            </section>
          </>
        )}

        <Separator />

        <section>
          <h3 className="mb-2 flex items-center justify-between gap-2 text-[11px] font-medium"><span>Tags</span><span className="font-normal text-muted-foreground">{selectedCount.toLocaleString()} {selectedCount === 1 ? "asset" : "assets"}</span></h3>
          {visibleTags.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-1">
              {visibleTags.map(({ name, partial }) => (
                <Button
                  type="button"
                  key={name}
                  variant={partial ? "outline" : "secondary"}
                  size="xs"
                  className="h-6 rounded-sm px-1.5 text-[11px] font-normal"
                  onClick={() => void onRemoveTag(name)}
                  disabled={busy}
                  title={partial ? `Remove ${name} from every selected asset` : "Remove tag"}
                >
                  {name}
                  {partial && <span className="text-muted-foreground">some</span>}
                  <X className="size-2.5" />
                </Button>
              ))}
            </div>
          )}
          {selectedCount > 1 && !allSelectedKnown && <p className="mb-2 text-[11px] text-muted-foreground">Tag values are unavailable for part of the selection.</p>}
          <form className="flex gap-1.5" onSubmit={submitTag}>
            <Input
              ref={tagInputRef}
              value={tag}
              onChange={(event) => setTag(event.target.value)}
              placeholder={selectedCount > 1 ? `Add tag to ${selectedCount} assets` : "Add tag"}
              aria-label={selectedCount > 1 ? `Add tag to ${selectedCount} assets` : "Add tag"}
              className="h-7 rounded-sm text-xs"
              disabled={busy}
            />
            <Button type="submit" variant="outline" size="icon-sm" className="rounded-sm" disabled={busy}>
              <Plus />
              <span className="sr-only">Add tag</span>
            </Button>
          </form>
        </section>

        <Separator />

        <section>
          <h3 className="mb-1 flex items-center justify-between gap-2 text-[11px] font-medium"><span>Collections</span><span className="font-normal text-muted-foreground">{selectedCount.toLocaleString()} {selectedCount === 1 ? "asset" : "assets"}</span></h3>
          <div className="space-y-0.5">
            {collections.map((collection) => {
              const memberships = selectedAssets.map((item) => item.collectionIds.includes(collection.id));
              const included = selectedCount === 1
                ? asset.collectionIds.includes(collection.id)
                : allSelectedKnown && memberships.every(Boolean);
              const mixed = selectedCount > 1 && (!allSelectedKnown || (memberships.some(Boolean) && !memberships.every(Boolean)));
              return (
                <label
                  key={collection.id}
                  className="flex h-7 items-center gap-2 text-[11px] text-foreground/85"
                >
                  <Checkbox
                    checked={included}
                    indeterminate={mixed}
                    disabled={busy}
                    onCheckedChange={(checked) =>
                      void onMembership(collection.id, Boolean(checked))
                    }
                  />
                  <span className="truncate">{collection.name}</span>
                </label>
              );
            })}
            {collections.length === 0 && (
              <Button type="button" variant="outline" size="sm" className="mt-1" onClick={onAddCollection} disabled={busy}>
                <Plus /> New collection
              </Button>
            )}
          </div>
        </section>
      </div>
    </aside>
  );
}
