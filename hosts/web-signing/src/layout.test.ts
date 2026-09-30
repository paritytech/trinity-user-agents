import { describe, expect, test } from "bun:test";
import { Layout } from "./layout.js";

describe("Layout beside the product", () => {
  test("opens the menu and the inspector together", () => {
    const layout = new Layout(["menu"]);
    layout.show("inspector");
    expect([layout.isOpen("menu"), layout.isOpen("inspector")]).toEqual([
      true,
      true,
    ]);
  });

  // The panels do not cover the product here, so Escape has nothing to
  // dismiss and must not close a panel the user is working in.
  test("has nothing covering the product", () => {
    const layout = new Layout(["menu", "inspector"]);
    expect(layout.covering).toBeNull();
    expect(layout.dismissCovering()).toBe(false);
    expect(layout.isOpen("menu")).toBe(true);
  });
});

describe("Layout over the product", () => {
  test("opens one panel at a time", () => {
    const layout = new Layout();
    layout.setModal(true);
    layout.show("menu");
    layout.show("inspector");
    expect(layout.isOpen("menu")).toBe(false);
    expect(layout.covering).toBe("inspector");
  });

  test("dismisses the covering panel", () => {
    const layout = new Layout();
    layout.setModal(true);
    layout.show("menu");
    expect(layout.dismissCovering()).toBe(true);
    expect(layout.covering).toBeNull();
  });

  // Turning a phone sideways or resizing a window must not leave two
  // covering panels stacked on the product.
  test("keeps one panel when the screen narrows", () => {
    const layout = new Layout(["menu", "inspector"]);
    layout.setModal(true);
    expect(layout.covering).toBe("menu");
    expect(layout.isOpen("inspector")).toBe(false);
  });

  test("toggle closes an open panel", () => {
    const layout = new Layout(["menu"]);
    layout.toggle("menu");
    expect(layout.isOpen("menu")).toBe(false);
  });
});
