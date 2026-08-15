import { beforeEach, describe, expect, it } from "vitest";
import { readProjectModelFormats, writeProjectModelFormats } from "./godotExportPreferences";

describe("Godot model format preferences", () => {
  let values: Map<string, string>;
  let storage: Pick<Storage, "getItem" | "setItem">;

  beforeEach(() => {
    values = new Map();
    storage = {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value); },
    };
  });

  it("keeps a normalized choice isolated to each project", () => {
    writeProjectModelFormats(4, ["GLB", "obj", "glb"], storage);
    expect(readProjectModelFormats(4, storage)).toEqual(["glb", "obj"]);
    expect(readProjectModelFormats(5, storage)).toBeNull();
  });

  it("falls back to the all-formats default when saved data is unusable", () => {
    storage.setItem("lootbox:godot-model-formats:4", "not-json");
    expect(readProjectModelFormats(4, storage)).toBeNull();
  });
});
