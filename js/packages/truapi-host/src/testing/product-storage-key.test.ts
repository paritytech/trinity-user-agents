import { describe, expect, it } from "bun:test";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

import {
  coreProductStorageKey,
  mockTestStack,
  type TestStack,
} from "../web/create-mock-host.js";
import { wasmArtifact, wasmIsBuilt } from "./require-wasm.js";

const suite = wasmIsBuilt("testing/truapi_server.js")
  ? describe
  : describe.skip;

/** The core's seeding helpers, loaded the way `productAccountAddress` loads its own. */
async function seedingHelpers() {
  const glue = (await import(
    pathToFileURL(wasmArtifact("testing/truapi_server.js")).href
  )) as {
    default: (init: { module_or_path: Uint8Array }) => Promise<unknown>;
    productStorageKey(productId: string, key: string): string;
    testStackServedProductId(testStack: TestStack, productId: string): string;
  };
  await glue.default({
    module_or_path: await readFile(
      wasmArtifact("testing/truapi_server_bg.wasm"),
    ),
  });
  return glue;
}

async function productStorageKey(
  productId: string,
  key: string,
): Promise<string> {
  return (await seedingHelpers()).productStorageKey(productId, key);
}

suite("the key a seeded product storage entry is stored under", () => {
  it("is the key the core reads that entry by", async () => {
    // The format is the host callback ABI, so a seed that rebuilt it any other
    // way would land where the core never looks.
    expect(await productStorageKey("localhost:5173", "browse:labels")).toBe(
      "truapi:product-storage:v1:14:localhost:5173:browse:labels",
    );
  });

  it("reads back as the product's own key", async () => {
    const stored = await productStorageKey("calc.dot", "mykey");
    expect(coreProductStorageKey(stored)).toBe("mykey");
  });

  it("refuses a product id the core would refuse", async () => {
    await expect(productStorageKey("", "mykey")).rejects.toThrow(
      "product_id must not be empty",
    );
  });

  it("names the product as the host's stack serves it", async () => {
    // The core runs `calc.dot` as `calc.paseo` on the mock stack and reads
    // its storage by that id, so a seed keyed by the id as written would
    // land where the product never looks.
    const { testStackServedProductId } = await seedingHelpers();
    const onDot = {
      ...mockTestStack(),
      tld: "dot",
      reservedValues: { allowed: true },
    };
    expect([
      testStackServedProductId(mockTestStack(), "calc.dot"),
      testStackServedProductId(mockTestStack(), "localhost:5173"),
      testStackServedProductId(onDot, "calc.dot"),
    ]).toEqual(["calc.paseo", "localhost:5173", "calc.dot"]);
    expect(() =>
      testStackServedProductId(mockTestStack(), "calc.testnet"),
    ).toThrow("the paseo stack does not serve product calc.testnet");
  });
});
