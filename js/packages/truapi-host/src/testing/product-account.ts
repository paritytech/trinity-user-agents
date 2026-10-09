// The address a product account will be given, worked out without a host.
//
// A product account is derived from (session root, product id), so it exists
// before any host runs and is the same on every run. That is what lets a suite
// fund it once, in a `globalSetup`, rather than per test: funding is a property
// of the account, not of the run.

import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

import { DerivationIndex } from "@parity/truapi";

import {
  checkDerivationIndex,
  resolveAccount,
  type DevAccount,
  type DevAccountName,
} from "./dev-accounts.js";
import { wasmArtifact } from "./require-wasm.js";

/** Which account, product and index to derive. */
export interface ProductAccountQuery {
  /** The dev account whose session root the product account descends from. */
  account: DevAccountName | DevAccount;
  /** The product's dotNS identifier, e.g. `"tx-demo.dot"`. */
  productId: string;
  /** Index within the product's subtree. Defaults to `0`. */
  index?: number;
}

/** The core's pure derivation helpers, as the node bundle exports them. */
interface Derivation {
  default: (init: { module_or_path: Uint8Array }) => Promise<unknown>;
  deriveProductSubtreePublicKey(
    rootEntropy: Uint8Array,
    productId: string,
  ): Uint8Array;
  deriveProductAccountPublicKey(
    productSubtreePublicKey: Uint8Array,
    derivationIndex: Uint8Array,
  ): Uint8Array;
  productAccountAddress(publicKey: Uint8Array): string;
}

let loaded: Promise<Derivation> | undefined;

/** Load the core's wasm once, for the derivation helpers alone. */
async function derivation(): Promise<Derivation> {
  loaded ??= (async () => {
    const glue = (await import(
      // A file URL rather than a path: node refuses a Windows drive letter as
      // an ESM specifier.
      /* @vite-ignore */ pathToFileURL(wasmArtifact("testing/truapi_server.js")).href
    )) as Derivation;
    // The bytes are handed over rather than left to the glue's own loader,
    // which fetches a URL: node has none to fetch, and a suite reaches this
    // from a Playwright global setup, which node runs.
    await glue.default({
      module_or_path: await readFile(wasmArtifact("testing/truapi_server_bg.wasm")),
    });
    return glue;
  })();
  return loaded;
}

/**
 * The SS58 address of the product account `query` names.
 *
 * Encoded at the prefix the core mandates, which is not necessarily the one a
 * product displays. Pass it to a faucet or a transfer as it stands.
 *
 * Needs the built WASM bundle; see `wasmIsBuilt`.
 */
export async function productAccountAddress(
  query: ProductAccountQuery,
): Promise<string> {
  const { entropy } = resolveAccount(query.account);
  // The index crosses SCALE-encoded, so the chain code stays core-owned.
  const index = DerivationIndex.enc({
    tag: "Index",
    value: checkDerivationIndex(query.index ?? 0),
  });
  const core = await derivation();
  const subtree = core.deriveProductSubtreePublicKey(entropy, query.productId);
  return core.productAccountAddress(
    core.deriveProductAccountPublicKey(subtree, index),
  );
}
