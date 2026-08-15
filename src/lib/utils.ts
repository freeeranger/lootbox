import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
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
