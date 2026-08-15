import { describe, expect, it } from "vitest";
import { assetListRowHeight, isAssetKeyboardTarget } from "./workspaceShortcuts";

describe("workspace keyboard scope", () => {
  it("accepts the asset browser and document body", () => {
    const browser = document.createElement("div");
    browser.dataset.assetBrowser = "";
    const card = document.createElement("button");
    card.setAttribute("role", "option");
    browser.appendChild(card);
    document.body.appendChild(browser);
    expect(isAssetKeyboardTarget(card, card)).toBe(true);
    expect(isAssetKeyboardTarget(document.body, document.body)).toBe(true);
    browser.remove();
  });

  it("does not intercept focused toolbar controls", () => {
    const toolbarButton = document.createElement("button");
    document.body.appendChild(toolbarButton);
    toolbarButton.focus();
    expect(isAssetKeyboardTarget(toolbarButton, toolbarButton)).toBe(false);
    toolbarButton.remove();
  });

  it("keeps controls layered over an asset card independent from browser shortcuts", () => {
    const browser = document.createElement("div");
    browser.dataset.assetBrowser = "";
    const card = document.createElement("div");
    card.dataset.assetCard = "";
    const option = document.createElement("button");
    option.setAttribute("role", "option");
    const audioControl = document.createElement("button");
    card.append(option, audioControl);
    browser.appendChild(card);
    document.body.appendChild(browser);

    expect(isAssetKeyboardTarget(option, option)).toBe(true);
    expect(isAssetKeyboardTarget(audioControl, audioControl)).toBe(false);
    browser.remove();
  });

  it("matches the rendered list row geometry", () => {
    expect(assetListRowHeight).toBe(48);
  });
});
