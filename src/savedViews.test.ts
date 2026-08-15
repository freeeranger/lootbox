import { beforeEach, describe, expect, it } from "vitest";
import { readSavedViews, resolveSavedSelection, writeSavedViews } from "./savedViews";

describe("saved views", () => {
  const values = new Map<string, string>();
  beforeEach(() => {
    values.clear();
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value),
      },
    });
  });

  it("round trips a saved asset view", () => {
    const view = {
      id: "recent-models",
      name: "Recent models",
      query: "",
      filters: {
        type: "model",
        extension: "glb",
        mapRole: "",
        tag: "",
        minWidth: "",
        minConfidence: "",
        status: "",
        projectUsage: "",
      },
      sort: "newest" as const,
      sortDirection: "desc" as const,
      selection: { kind: "all" as const },
    };
    writeSavedViews([view]);
    expect(readSavedViews()).toEqual([view]);
  });

  it("ignores malformed storage", () => {
    window.localStorage.setItem("lootbox:saved-views", "not-json");
    expect(readSavedViews()).toEqual([]);
  });

  it("rejects invalid sort values, directions, selection kinds, and ids", () => {
    const base = {
      id: "saved",
      name: "Saved",
      query: "",
      filters: {},
      sort: "name",
      sortDirection: "asc",
      selection: { kind: "all" },
    };
    window.localStorage.setItem("lootbox:saved-views", JSON.stringify([
      { ...base, id: "bad-sort", sort: "random" },
      { ...base, id: "bad-direction", sortDirection: "sideways" },
      { ...base, id: "bad-kind", selection: { kind: "planet" } },
      { ...base, id: "bad-project", selection: { kind: "project", projectId: 0 } },
      { ...base, id: "good-project", selection: { kind: "project", projectId: 7 } },
    ]));

    expect(readSavedViews().map((view) => view.id)).toEqual(["good-project"]);
  });

  it("normalizes filters written before project usage was added", () => {
    window.localStorage.setItem("lootbox:saved-views", JSON.stringify([{
      id: "legacy",
      name: "Legacy",
      query: "stone",
      filters: { extension: "png" },
      sort: "name",
      sortDirection: "asc",
      selection: { kind: "all" },
    }]));

    expect(readSavedViews()[0].filters).toMatchObject({ extension: "png", projectUsage: "", status: "" });
  });

  it("falls back safely when a saved project is no longer registered", () => {
    expect(resolveSavedSelection({ kind: "project", projectId: 7 }, new Set([8]))).toEqual({
      selection: { kind: "all" },
      staleProject: true,
    });
    expect(resolveSavedSelection({ kind: "project", projectId: 7 }, new Set([7]))).toEqual({
      selection: { kind: "project", projectId: 7 },
      staleProject: false,
    });
  });
});
