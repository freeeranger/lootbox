import { describe, expect, it } from "vitest";
import { godotExportCompletionCopy } from "./godotExportCompletion";

describe("Godot export completion copy", () => {
  it("states plainly when new files were exported", () => {
    expect(godotExportCompletionCopy("Tiny Game", {
      copied: 1,
      unchanged: 0,
      destination: "res://assets/lootbox",
    })).toEqual({
      title: "Exported to Tiny Game",
      message: "1 file copied to res://assets/lootbox.",
    });
  });

  it("distinguishes an already-current export from a new copy", () => {
    expect(godotExportCompletionCopy("Tiny Game", {
      copied: 0,
      unchanged: 2,
      destination: "res://assets/lootbox",
    }).title).toBe("Already exported to Tiny Game");
  });

  it("reports mixed copied and current files", () => {
    expect(godotExportCompletionCopy("Tiny Game", {
      copied: 2,
      unchanged: 3,
      destination: "res://assets/lootbox",
    }).message).toBe("2 files copied to res://assets/lootbox; 3 files already current.");
  });
});
