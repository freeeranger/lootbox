import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

const naturalCollator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

/**
 * Robust natural string comparator taking numeric sequences into account.
 * E.g. "pack vol 9" comes before "pack vol 56".
 */
export function compareNatural(a?: string | null, b?: string | null): number {
  if (a === b) return 0;
  if (a == null) return -1;
  if (b == null) return 1;
  return naturalCollator.compare(a, b);
}

/**
 * Returns a new array sorted naturally by the string returned by keyFn (or the item itself if string).
 */
export function sortByNatural<T>(
  items: readonly T[],
  keyFn?: (item: T) => string | null | undefined,
): T[] {
  if (keyFn) {
    return [...items].sort((a, b) => compareNatural(keyFn(a), keyFn(b)));
  }
  return [...items].sort((a, b) => compareNatural(a as unknown as string, b as unknown as string));
}


/**
 * Collapses user home directory paths to '~' for cleaner UI presentation.
 * Supports Linux (/home/user), macOS (/Users/user), and Windows (C:\Users\user).
 * E.g. '/home/alex/dev/game' -> '~/dev/game'
 *      '/home/alex' -> '~'
 */
export function collapseHomePath(path: string | null | undefined): string {
  if (!path) return "";
  // Match Linux /home/username or macOS /Users/username
  const unixMatch = path.match(/^(?:\/home|\/Users)\/[^/]+(?:\/(.*)|$)/);
  if (unixMatch) {
    const sub = unixMatch[1];
    return sub !== undefined ? `~/${sub}` : "~";
  }
  // Match Windows C:\Users\username or C:/Users/username
  const winMatch = path.match(/^[A-Za-z]:[\\/]Users[\\/][^\\/]+(?:[\\/](.*)|$)/);
  if (winMatch) {
    const sub = winMatch[1];
    return sub !== undefined ? `~\\${sub}` : "~";
  }
  return path;
}

/**
 * Truncates text in the middle with an ellipsis, preserving the beginning and ending.
 * Useful for long filenames, stems, and identifiers (e.g. 'space_corridor_straight' -> 'space_co...straight').
 */
export function truncateMiddle(text: string | null | undefined, maxLength = 24): string {
  if (!text) return "";
  if (text.length <= maxLength) return text;
  const charsToShow = Math.max(maxLength - 3, 2);
  const frontChars = Math.ceil(charsToShow / 2);
  const backChars = Math.floor(charsToShow / 2);
  return `${text.slice(0, frontChars)}...${text.slice(text.length - backChars)}`;
}

export function formatTriangles(count: number | null | undefined): string {
  if (count == null || count <= 0) return "";
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M tris`;
  if (count >= 10_000) return `${Math.round(count / 1_000)}k tris`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k tris`;
  return `${count.toLocaleString()} tris`;
}

export function formatVertices(count: number | null | undefined): string {
  if (count == null || count <= 0) return "";
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M verts`;
  if (count >= 10_000) return `${Math.round(count / 1_000)}k verts`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k verts`;
  return `${count.toLocaleString()} verts`;
}

export interface AssetSpecs {
  primary: string | null;
  secondary: string | null;
}

export function getAssetSpecs(asset: {
  assetType: string;
  width?: number | null;
  height?: number | null;
  resolution?: string | null;
  mapRole?: string | null;
  triangles?: number | null;
  vertices?: number | null;
}): AssetSpecs {
  if (asset.assetType === "image" || asset.assetType === "texture") {
    const dims = asset.width && asset.height ? `${asset.width} × ${asset.height}` : null;
    const parts = [
      asset.resolution,
      asset.mapRole ? asset.mapRole.replaceAll("_", " ") : null,
    ].filter(Boolean);
    const detail = parts.length > 0 ? parts.join(" · ") : null;

    if (dims && detail) {
      return { primary: dims, secondary: detail };
    }
    if (dims) {
      return { primary: dims, secondary: null };
    }
    if (detail) {
      return { primary: detail, secondary: null };
    }
    return { primary: null, secondary: null };
  }

  if (asset.assetType === "model") {
    if (asset.triangles != null && asset.triangles > 0) {
      const tris = formatTriangles(asset.triangles);
      const verts = asset.vertices != null && asset.vertices > 0 ? formatVertices(asset.vertices) : null;
      return { primary: tris, secondary: verts };
    }
    return { primary: null, secondary: null };
  }

  if (asset.assetType === "video" && asset.width && asset.height) {
    return { primary: `${asset.width} × ${asset.height}`, secondary: null };
  }

  if (asset.mapRole) {
    return {
      primary: asset.mapRole.replaceAll("_", " "),
      secondary: asset.resolution || null,
    };
  }

  return { primary: null, secondary: null };
}
