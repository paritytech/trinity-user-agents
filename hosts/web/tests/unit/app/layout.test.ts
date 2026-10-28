import { describe, expect, test } from "bun:test";
import { Layout } from "../../../src/app/layout.js";

describe("Layout beside the app", () => {
  test("a toggle shows its view, and closes the tools when its view already shows", () => {
    const layout = new Layout(true);
    layout.toggle("inspector");
    expect([layout.open, layout.view]).toEqual([true, "inspector"]);
    layout.toggle("inspector");
    expect(layout.open).toBe(false);
  });

  // Focus hides the tools and keeps the view, so they come back as they were.
  test("Focus closes the tools and opens them again at the same view", () => {
    const layout = new Layout(true, "inspector");
    layout.toggleOpen();
    expect(layout.isShown("inspector")).toBe(false);
    layout.toggleOpen();
    expect(layout.isShown("inspector")).toBe(true);
  });

  // The tools do not cover the app here, so Escape has nothing to dismiss and
  // must not close tools the user is working in.
  test("has nothing covering the app", () => {
    const layout = new Layout(true);
    expect(layout.covering).toBe(false);
    expect(layout.dismissCovering()).toBe(false);
    expect(layout.open).toBe(true);
  });
});

describe("Layout over the app", () => {
  test("dismisses the open tools", () => {
    const layout = new Layout(false);
    layout.setModal(true);
    layout.show("account");
    expect(layout.covering).toBe(true);
    expect(layout.dismissCovering()).toBe(true);
    expect(layout.covering).toBe(false);
  });
});
