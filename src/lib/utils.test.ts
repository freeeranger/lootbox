import { describe, expect, it } from "vitest";
import { collapseHomePath, truncateMiddle } from "./utils";

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
