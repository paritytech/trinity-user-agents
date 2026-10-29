// What the package's tests share: the crate's conformance vectors, a signed test stack, and a
// dotNS transport over a vector's chain. Excluded from the build.
import { readFile } from "node:fs/promises";
import type { Block } from "../wasm/trinity_stack.js";
import { type DotnsTransport, DotnsRevert } from "./dotns.js";
import { type Model, generateSecret, publicKey, publish } from "./index.js";

const VECTORS = new URL(
  "../../../../rust/crates/trinity-stack/vectors/",
  import.meta.url,
);

/** The crate's `vectors/cases.json`. */
export const cases = JSON.parse(
  await readFile(new URL("cases.json", VECTORS), "utf8"),
);

/** A file the vectors name, or undefined for null. */
export async function file(path: string | null) {
  return path === null
    ? undefined
    : new Uint8Array(await readFile(new URL(path, VECTORS)));
}

/** A vector's pointer, with a null key left out. */
export function pointerOf(pointer: { url: string; key: string | null }) {
  return { url: pointer.url, key: pointer.key ?? undefined };
}

export const URL_ = "https://stack.example/.well-known/trinity-stack.json";

export const MODEL: Model = {
  id: "local",
  name: "Local",
  network: "local",
  tld: "local",
  chains: [
    {
      role: "Relay",
      genesisHash: `0x${"86".repeat(32)}`,
      rpc: ["ws://127.0.0.1:9944"],
    },
    {
      role: "AssetHub",
      genesisHash: `0x${"bb".repeat(32)}`,
      paraId: 1000,
      rpc: ["ws://127.0.0.1:9945"],
    },
  ],
  services: [],
};

const secret = generateSecret();

/** The test stack's pointer: `URL_`, signed by this run's key. */
export const pointer = { url: URL_, key: publicKey(secret) };

/** The test stack as published at second `seconds`, so a later one is newer. */
export async function publishedAt(
  seconds: number,
  generation = "g1",
  model = MODEL,
) {
  const { document } = await publish(model, {
    generation,
    secret,
    now: new Date(seconds * 1000),
  });
  // A copy over a plain ArrayBuffer, which a Response body takes.
  return document.slice();
}

/** A dotNS vector's chain: the gateway's storage and each call's answer, null for a revert. */
export type DotnsChain = {
  at: Block;
  storageKey: string;
  storage: string | null;
  calls: Record<string, string | null>;
};

/**
 * Asset Hub as a dotNS vector has it. `onRead` sees the block of every storage read and call,
 * and `at` overrides the block a lookup reads at.
 */
export function transportOf(
  chain: DotnsChain,
  options: { at?: () => Promise<Block>; onRead?: (at: Block) => void } = {},
): DotnsTransport {
  return {
    at: options.at ?? (async () => chain.at),
    storage: async (key, at) => {
      if (key !== chain.storageKey) throw new Error(`unexpected key ${key}`);
      options.onRead?.(at);
      return chain.storage;
    },
    call: async (to, data, at) => {
      options.onRead?.(at);
      const answer = chain.calls[`${to}:${data}`];
      if (answer === undefined)
        throw new Error(`unexpected call ${to}:${data}`);
      if (answer === null) throw new DotnsRevert("execution reverted");
      return answer;
    },
  };
}
