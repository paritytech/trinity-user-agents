import { describe, expect, it } from "bun:test";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

import { coreProductStorageKey } from "../web/create-mock-host.js";
import { wasmArtifact, wasmIsBuilt } from "./require-wasm.js";

const suite = wasmIsBuilt("testing/truapi_server.js")
  ? describe
  : describe.skip;

/** The core's storage key encoder, loaded the way `productAccountAddress` loads its helpers. */
async function productStorageKey(
  productId: string,
  key: string,
): Promise<string> {
  const glue = (await import(
    pathToFileURL(wasmArtifact("testing/truapi_server.js")).href
  )) as {
    default: (init: { module_or_path: Uint8Array }) => Promise<unknown>;
    productStorageKey(productId: string, key: string): string;
  };
  await glue.default({
    module_or_path: await readFile(
      wasmArtifact("testing/truapi_server_bg.wasm"),
    ),
  });
  return glue.productStorageKey(productId, key);
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
});
