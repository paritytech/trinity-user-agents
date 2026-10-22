import { describe, expect, test } from "bun:test";
import {
  deviceLine,
  productStatusText,
  type Opened,
  type ProductView,
} from "./product-status.js";

const URL_OPENED: Opened = { address: "http://localhost:3000/", via: "url" };
const NAME_OPENED: Opened = { address: "app.paseo", via: "name", kind: "car" };
const VIEW: ProductView = { url: new URL("http://localhost:3000/") };

describe("device line", () => {
  test("says the browser and the OS ask", () => {
    expect(deviceLine(true)).toBe("Camera/mic: the browser and the OS ask.");
  });

  test("names the secure-context requirement", () => {
    expect(deviceLine(false)).toContain("https or localhost");
  });
});

describe("product status text", () => {
  test("shows the address and the device line of a URL product", () => {
    expect(productStatusText(VIEW, URL_OPENED, "", true).split("\n")).toEqual([
      "http://localhost:3000/",
      "Camera/mic: the browser and the OS ask.",
    ]);
  });

  test("shows the sandbox status of a product that is starting", () => {
    const text = productStatusText(
      VIEW,
      NAME_OPENED,
      "Sandbox: Fetching.",
      true,
    );
    expect(text.split("\n")).toEqual([
      "app.paseo (app archive)",
      "Sandbox: Fetching.",
      "Camera/mic: the browser and the OS ask.",
    ]);
  });
});
