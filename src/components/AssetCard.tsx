import { lazy, memo, Suspense, useState, useSyncExternalStore } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Check, Clipboard, Copy, ExternalLink, FolderOpen, LoaderCircle, Pause, Play, RotateCcw, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { AssetTypeIcon } from "./AssetTypeIcon";
import type { Asset } from "../types";
import {
  getAudioPlaybackSnapshot,
  subscribeAudioPlayback,
  toggleAudioPlayback,
} from "../audioPlayback";

const ModelCardPreview = lazy(() =>
  import("./ModelCardPreview").then((module) => ({ default: module.ModelCardPreview })),
);

const browserImages = new Set(["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"]);

function formatBytes(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

interface Props {
  asset: Asset;
  selected: boolean;
  view: "grid" | "list";
  onSelect: (asset: Asset, event: React.MouseEvent<HTMLButtonElement>) => void;
  onContextSelect: (asset: Asset) => void;
  onOpen: (asset: Asset) => void;
  onReveal: (asset: Asset) => void;
  onRemove: (asset: Asset) => void;
  onRestore: (asset: Asset) => void;
  removed: boolean;
  selectionCount: number;
  dragPaths: string[];
  onCopyPath: (path: string) => void;
  onError: (error: unknown) => void;
}

function AudioCardControl({
  asset,
  view,
  onError,
}: {
  asset: Asset;
  view: "grid" | "list";
  onError: (error: unknown) => void;
}) {
  const [busy, setBusy] = useState(false);
  const playback = useSyncExternalStore(
    subscribeAudioPlayback,
    getAudioPlaybackSnapshot,
  );
  const playing = playback.path === asset.absolutePath && playback.playing;

  return (
    <button
      type="button"
      className={cn(
        "absolute z-10 grid place-items-center rounded-full border border-border/70 bg-background/85 text-foreground shadow-sm backdrop-blur-sm hover:bg-accent",
        view === "grid" ? "top-2 left-2 size-7" : "top-2 left-2 size-7",
      )}
      aria-label={playing ? `Pause ${asset.name}` : `Play ${asset.name}`}
      disabled={busy}
      onClick={(event) => {
        event.stopPropagation();
        setBusy(true);
        void toggleAudioPlayback(asset.absolutePath)
          .catch(onError)
          .finally(() => setBusy(false));
      }}
    >
      {busy ? (
        <LoaderCircle className="size-3 animate-spin" />
      ) : playing ? (
        <Pause className="size-3" />
      ) : (
        <Play className="size-3" />
      )}
    </button>
  );
}

function AssetCardComponent({
  asset,
  selected,
  view,
  onSelect,
  onContextSelect,
  onOpen,
  onReveal,
  onRemove,
  onRestore,
  removed,
  selectionCount,
  dragPaths,
  onCopyPath,
  onError,
}: Props) {
  const imageSource =
    asset.thumbnailPath ??
    (["image", "texture"].includes(asset.assetType) && browserImages.has(asset.extension)
      ? asset.absolutePath
      : null);

  function startDrag(event: React.DragEvent<HTMLButtonElement>) {
    const paths = dragPaths.length > 0 ? dragPaths : [asset.absolutePath];
    const uris = paths.map((path) => {
      const normalizedPath = path.replaceAll("\\", "/");
      return `file://${normalizedPath
        .split("/")
        .map((part) => encodeURIComponent(part))
        .join("/")}`;
    });
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("text/plain", paths.join("\n"));
    event.dataTransfer.setData("text/uri-list", uris.join("\r\n"));
    event.dataTransfer.setData(
      "DownloadURL",
      `application/octet-stream:${asset.name}.${asset.extension}:${uris[0]}`,
    );
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger className="contents">
        <div className="relative min-w-0" data-asset-card>
          <button
            type="button"
            className={cn(
              "group block w-full min-w-0 rounded-md text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60",
              view === "list" &&
                "grid h-12 grid-cols-[34px_minmax(130px,0.5fr)_minmax(0,1fr)_84px] items-center gap-3 px-2 hover:bg-accent/55",
              view === "list" && selected && "bg-primary/10 ring-1 ring-inset ring-primary/25",
              view === "grid" && "p-1.5 hover:bg-accent/35",
              view === "grid" && selected && "bg-primary/10 ring-1 ring-inset ring-primary/30",
            )}
            onClick={(event) => onSelect(asset, event)}
            onContextMenu={() => onContextSelect(asset)}
            onDoubleClick={() => onOpen(asset)}
            onDragStart={startDrag}
            draggable
            aria-pressed={selected}
            title={asset.relativePath}
          >
      <span
        className={cn(
          "relative block overflow-hidden border bg-muted/20",
          view === "grid" ? "aspect-[4/3] rounded-md" : "size-[34px] rounded-md",
          selected && view === "grid" && "border-primary/60",
          !selected && "group-hover:border-foreground/20",
          imageSource && ["image", "texture"].includes(asset.assetType) && "checkerboard",
        )}
      >
        {imageSource ? (
          <img
            src={convertFileSrc(imageSource)}
            alt=""
            loading="lazy"
            decoding="async"
            className={cn("size-full object-contain", view === "grid" && "p-1")}
          />
        ) : asset.assetType === "model" && ["glb", "gltf"].includes(asset.extension) ? (
          <Suspense
            fallback={
              <span className="grid size-full place-items-center text-muted-foreground/65">
                <AssetTypeIcon type="model" size={view === "grid" ? 30 : 15} />
              </span>
            }
          >
            <ModelCardPreview asset={asset} iconSize={view === "grid" ? 30 : 15} onError={onError} />
          </Suspense>
        ) : (
          <span className="grid size-full place-items-center text-muted-foreground/65">
            <AssetTypeIcon type={asset.assetType} size={view === "grid" ? 30 : 15} />
          </span>
        )}
      </span>

      {view === "grid" && selected && (
        <span className="absolute top-3 right-3 grid size-5 place-items-center rounded-full bg-primary text-primary-foreground shadow-sm">
          <Check className="size-3" strokeWidth={2.5} />
        </span>
      )}

      {view === "grid" && asset.duplicateCount > 1 && (
        <span className={cn("absolute left-3 flex h-5 items-center gap-1 rounded-full border bg-background/90 px-1.5 text-[11px] text-muted-foreground backdrop-blur-sm", asset.assetType === "audio" ? "top-11" : "top-3")} title={`${asset.duplicateCount} identical files`}>
          <Copy className="size-2.5" /> {asset.duplicateCount}
        </span>
      )}

      <span className={cn("min-w-0", view === "grid" && "mt-1.5 block px-0.5")}>
        <span className="block truncate text-xs font-medium text-foreground/90">{asset.name}</span>
        {view === "grid" && (
          <span className="mt-0.5 flex min-w-0 items-center gap-1 truncate text-[11px] text-muted-foreground">
            <span className="font-mono uppercase">{asset.extension || asset.assetType}</span>
            {asset.mapRole && <><span>·</span><span className="truncate capitalize">{asset.mapRole.replaceAll("_", " ")}</span></>}
            {asset.resolution && <><span>·</span><span>{asset.resolution}</span></>}
          </span>
        )}
      </span>

      {view === "list" && (
        <span className="min-w-0 truncate text-[11px] text-muted-foreground">
          {asset.relativePath}
        </span>
      )}
      {view === "list" && (
        <span className="min-w-0 text-right">
          <span className="block truncate font-mono text-[11px] uppercase text-foreground/65">{asset.extension || asset.assetType}</span>
          <span className="block text-[11px] text-muted-foreground">{formatBytes(asset.sizeBytes)}</span>
        </span>
      )}
          </button>
          {asset.assetType === "audio" && (
            <AudioCardControl asset={asset} view={view} onError={onError} />
          )}
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onClick={() => onOpen(asset)}>
          <ExternalLink /> Open
        </ContextMenuItem>
        <ContextMenuItem onClick={() => onReveal(asset)}>
          <FolderOpen /> Reveal in folder
        </ContextMenuItem>
        <ContextMenuItem onClick={() => onCopyPath(asset.absolutePath)}>
          <Clipboard /> Copy path
        </ContextMenuItem>
        <ContextMenuSeparator />
        {removed ? (
          <ContextMenuItem onClick={() => onRestore(asset)}>
            <RotateCcw /> {selectionCount > 1 ? `Restore ${selectionCount} assets` : "Restore to Lootbox"}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem variant="destructive" onClick={() => onRemove(asset)}>
            <Trash2 /> {selectionCount > 1 ? `Remove ${selectionCount} assets` : "Remove from Lootbox"}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}

export const AssetCard = memo(AssetCardComponent);
