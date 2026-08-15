import type { GodotExportResult } from "./types";

function filesLabel(count: number) {
  return `${count.toLocaleString()} ${count === 1 ? "file" : "files"}`;
}

export function godotExportCompletionCopy(
  projectName: string,
  result: GodotExportResult,
) {
  if (result.copied === 0 && result.unchanged > 0) {
    return {
      title: `Already exported to ${projectName}`,
      message: `${filesLabel(result.unchanged)} already current in ${result.destination}.`,
    };
  }
  if (result.unchanged > 0) {
    return {
      title: `Exported to ${projectName}`,
      message: `${filesLabel(result.copied)} copied to ${result.destination}; ${filesLabel(result.unchanged)} already current.`,
    };
  }
  return {
    title: `Exported to ${projectName}`,
    message: `${filesLabel(result.copied)} copied to ${result.destination}.`,
  };
}
