import { describe, expect, it } from "vitest";
import { installNativeShellBehavior } from "./nativeShell";

function contextMenu(target: Element) {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
}

function dragStart(target: Element) {
  const event = new Event("dragstart", { bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
}

describe("native shell behavior", () => {
  it("suppresses the browser context menu outside editable controls", () => {
    const uninstall = installNativeShellBehavior();
    const label = document.body.appendChild(document.createElement("span"));
    const input = document.body.appendChild(document.createElement("input"));

    expect(contextMenu(label).defaultPrevented).toBe(true);
    expect(contextMenu(input).defaultPrevented).toBe(false);
    uninstall();
  });

  it("preserves application context menus", () => {
    const uninstall = installNativeShellBehavior();
    const trigger = document.body.appendChild(document.createElement("div"));
    trigger.setAttribute("data-slot", "context-menu-trigger");
    const child = trigger.appendChild(document.createElement("button"));

    expect(contextMenu(child).defaultPrevented).toBe(false);
    uninstall();
  });

  it("blocks browser drag ghosts but preserves asset drag-out", () => {
    const uninstall = installNativeShellBehavior();
    const image = document.body.appendChild(document.createElement("img"));
    const card = document.body.appendChild(document.createElement("div"));
    card.setAttribute("data-asset-card", "");
    const cardImage = card.appendChild(document.createElement("img"));

    expect(dragStart(image).defaultPrevented).toBe(true);
    expect(dragStart(cardImage).defaultPrevented).toBe(false);
    uninstall();
  });

  it("blocks page shortcuts without consuming application shortcuts", () => {
    const uninstall = installNativeShellBehavior();
    const reload = new KeyboardEvent("keydown", { key: "r", ctrlKey: true, cancelable: true });
    const search = new KeyboardEvent("keydown", { key: "f", ctrlKey: true, cancelable: true });

    window.dispatchEvent(reload);
    window.dispatchEvent(search);
    expect(reload.defaultPrevented).toBe(true);
    expect(search.defaultPrevented).toBe(false);
    uninstall();
  });
});
