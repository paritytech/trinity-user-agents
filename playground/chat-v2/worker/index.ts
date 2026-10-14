/**
 * The Worker of the Chat Extension v2 sample product. The product logic is in
 * `product.ts`, so a test can start it with the client of a test host.
 */
import { getClientSync } from "@parity/truapi/sandbox";
import { loadAssets } from "./assets";
import { startSampleProduct } from "./product";

const client = getClientSync();
if (!client) {
  throw new Error("The Chat Extension v2 sample Worker needs a host connection");
}

startSampleProduct({ client, assets: loadAssets() });
