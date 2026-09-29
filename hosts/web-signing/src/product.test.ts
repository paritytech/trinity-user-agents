import { describe, expect, test } from "bun:test";
import { parseProductUrl, productIdFor } from "./product.js";

describe("productIdFor", () => {
  // Two local dev servers are two products, so they must not share accounts,
  // grants or storage.
  test("keeps the port of a localhost product", () => {
    expect(productIdFor(new URL("http://localhost:3000/app"), "")).toBe(
      "localhost:3000",
    );
    expect(productIdFor(new URL("http://localhost:3001/"), "")).toBe(
      "localhost:3001",
    );
  });

  test("uses the hostname of any other URL", () => {
    expect(productIdFor(new URL("https://example.test/"), "")).toBe(
      "example.test",
    );
  });

  test("lets an override name the dotNS product a URL stands in for", () => {
    expect(
      productIdFor(new URL("http://localhost:3000/"), " myapp.paseo "),
    ).toBe("myapp.paseo");
  });
});

describe("parseProductUrl", () => {
  test("accepts http and https", () => {
    expect(parseProductUrl(" http://localhost:3000 ").href).toBe(
      "http://localhost:3000/",
    );
  });

  test("refuses other schemes", () => {
    expect(() => parseProductUrl("javascript:alert(1)")).toThrow(
      "Only http and https",
    );
  });

  test("refuses text that is not a URL", () => {
    expect(() => parseProductUrl("myapp.paseo")).toThrow("Not a URL");
  });
});
