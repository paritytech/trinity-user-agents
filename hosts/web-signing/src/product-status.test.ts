import { describe, expect, test } from "bun:test";
import {
  containerLine,
  deviceLine,
  productStatusText,
  type Opened,
  type ProductView,
} from "./product-status.js";

const URL_OPENED: Opened = { address: "http://localhost:3000/", via: "url" };
const NAME_OPENED: Opened = { address: "app.paseo", via: "name", kind: "car" };

function view(overrides: Partial<ProductView> = {}): ProductView {
  return {
    url: new URL("http://localhost:3000/"),
    lost: false,
    container: null,
    ...overrides,
  };
}

describe("container line", () => {
  test("says on only while the container holds its port", () => {
    expect(
      containerLine(view({ container: { state: "connected" } }), NAME_OPENED),
    ).toBe("Container: on");
    expect(
      containerLine(view({ container: { state: "waiting" } }), NAME_OPENED),
    ).toBe("Container: starting");
  });

  // The bug this guards: after a hard navigation the container had a port
  // once, and the line kept saying it was on.
  test("does not say on for a product whose page loaded a new document", () => {
    const lost = view({ lost: true, container: { state: "lost" } });
    expect(containerLine(lost, NAME_OPENED)).toStartWith("Container: ended");
  });

  test("is blunt when nothing gates the page", () => {
    expect(containerLine(view(), URL_OPENED)).toContain("not gated");
    expect(containerLine(view(), NAME_OPENED)).toContain("relaxed");
  });
});

describe("device line", () => {
  test("offers devices only while the container can ask the core", () => {
    const connected = view({ container: { state: "connected" } });
    expect(deviceLine(connected, true)).toContain("the core asks");
    expect(deviceLine(view({ lost: true }), true)).toContain("reopened");
    expect(deviceLine(view(), true)).toBe("Camera/mic: blocked.");
  });

  test("names the secure-context requirement first", () => {
    const connected = view({ container: { state: "connected" } });
    expect(deviceLine(connected, false)).toContain("https or localhost");
  });
});

describe("product status text", () => {
  test("tells a lost product to reopen, above the container line", () => {
    const lost = view({ lost: true, container: { state: "lost" } });
    expect(productStatusText(lost, URL_OPENED, "", true).split("\n")).toEqual([
      "http://localhost:3000/",
      "The page navigated on its own, so the host's connection to it ended. Press Reopen.",
      "Container: ended. The page loaded a new document.",
      "Camera/mic: unavailable until the product is reopened.",
    ]);
  });

  test("shows the sandbox status of a product that is starting", () => {
    const text = productStatusText(
      view({ container: { state: "waiting" } }),
      NAME_OPENED,
      "Sandbox: Fetching.",
      true,
    );
    expect(text.split("\n")).toEqual([
      "app.paseo (app archive)",
      "Sandbox: Fetching.",
      "Container: starting",
      "Camera/mic: allowed, but only the browser asks until the container connects.",
    ]);
  });
});
