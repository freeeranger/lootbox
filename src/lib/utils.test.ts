import { describe, expect, it } from "vitest";
import { collapseHomePath, formatTriangles, formatVertices, getAssetSpecs, truncateMiddle } from "./utils";

describe("collapseHomePath", () => {
  it("collapses Linux /home/username paths", () => {
    expect(collapseHomePath("/home/freeranger/dev-2/lootbox")).toBe("~/dev-2/lootbox");
    expect(collapseHomePath("/home/freeranger")).toBe("~");
    expect(collapseHomePath("/home/freeranger/")).toBe("~/");
    expect(collapseHomePath("/home/usr/project/godot")).toBe("~/project/godot");
  });

  it("collapses macOS /Users/username paths", () => {
    expect(collapseHomePath("/Users/alex/projects/lootbox")).toBe("~/projects/lootbox");
    expect(collapseHomePath("/Users/alex")).toBe("~");
  });

  it("collapses Windows user paths", () => {
    expect(collapseHomePath("C:\\Users\\dev\\Games\\MyProject")).toBe("~\\Games\\MyProject");
    expect(collapseHomePath("D:\\Users\\artist")).toBe("~");
  });

  it("preserves non-home paths and handles empty/null inputs", () => {
    expect(collapseHomePath("/var/log/syslog")).toBe("/var/log/syslog");
    expect(collapseHomePath("/opt/godot/project")).toBe("/opt/godot/project");
    expect(collapseHomePath("")).toBe("");
    expect(collapseHomePath(null)).toBe("");
    expect(collapseHomePath(undefined)).toBe("");
  });
});

describe("truncateMiddle", () => {
  it("leaves short strings untruncated", () => {
    expect(truncateMiddle("sword.glb", 20)).toBe("sword.glb");
    expect(truncateMiddle("short_name", 24)).toBe("short_name");
  });

  it("truncates long strings in the middle while keeping front and back intact", () => {
    expect(truncateMiddle("space_corridor_straight_v2", 20)).toBe("space_cor...aight_v2");
    expect(truncateMiddle("stone_wall_brick_albedo_4k", 22)).toBe("stone_wall...albedo_4k");
  });

  it("handles null and empty input safely", () => {
    expect(truncateMiddle("")).toBe("");
    expect(truncateMiddle(null)).toBe("");
    expect(truncateMiddle(undefined)).toBe("");
  });
});

describe("formatTriangles and formatVertices", () => {
  it("formats triangle counts compactly", () => {
    expect(formatTriangles(520)).toBe("520 tris");
    expect(formatTriangles(1420)).toBe("1.4k tris");
    expect(formatTriangles(25000)).toBe("25k tris");
    expect(formatTriangles(1200000)).toBe("1.2M tris");
    expect(formatTriangles(0)).toBe("");
    expect(formatTriangles(null)).toBe("");
  });

  it("formats vertex counts compactly", () => {
    expect(formatVertices(860)).toBe("860 verts");
    expect(formatVertices(1890)).toBe("1.9k verts");
    expect(formatVertices(32000)).toBe("32k verts");
    expect(formatVertices(1500000)).toBe("1.5M verts");
    expect(formatVertices(0)).toBe("");
    expect(formatVertices(null)).toBe("");
  });
});

describe("getAssetSpecs", () => {
  it("extracts image and texture dimensions and resolution", () => {
    expect(getAssetSpecs({
      assetType: "image",
      width: 2048,
      height: 2048,
      resolution: "2K",
      mapRole: "normal_map",
    })).toEqual({
      primary: "2048 × 2048",
      secondary: "2K · normal map",
    });

    expect(getAssetSpecs({
      assetType: "image",
      width: 1920,
      height: 1080,
    })).toEqual({
      primary: "1920 × 1080",
      secondary: null,
    });
  });

  it("extracts model poly and vertex counts", () => {
    expect(getAssetSpecs({
      assetType: "model",
      triangles: 14200,
      vertices: 8940,
    })).toEqual({
      primary: "14k tris",
      secondary: "8.9k verts",
    });

    expect(getAssetSpecs({
      assetType: "model",
      triangles: null,
      vertices: null,
    })).toEqual({
      primary: null,
      secondary: null,
    });
  });
});
