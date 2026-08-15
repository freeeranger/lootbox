const projectModelFormatsKey = (projectId: number) =>
  `lootbox:godot-model-formats:${projectId}`;

type ProjectFormatStorage = Pick<Storage, "getItem" | "setItem">;

export function readProjectModelFormats(
  projectId: number,
  storage: ProjectFormatStorage = window.localStorage,
): string[] | null {
  try {
    const value = storage.getItem(projectModelFormatsKey(projectId));
    if (value === null) return null;
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed)) return null;
    const formats = [...new Set(parsed
      .filter((format): format is string => typeof format === "string")
      .map((format) => format.trim().replace(/^\./, "").toLowerCase())
      .filter(Boolean))];
    return formats.length > 0 ? formats : null;
  } catch {
    return null;
  }
}

export function writeProjectModelFormats(
  projectId: number,
  formats: string[],
  storage: ProjectFormatStorage = window.localStorage,
) {
  const normalized = [...new Set(formats.map((format) => format.toLowerCase()))].sort();
  try {
    storage.setItem(projectModelFormatsKey(projectId), JSON.stringify(normalized));
  } catch {
    // Preferences are optional; a storage failure must never block exporting.
  }
}
