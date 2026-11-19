// What the package's tests share: the crate's conformance vectors and a signed test stack.
// Excluded from the build.
import { readFile } from "node:fs/promises";
import { type Model, generateSecret, publicKey, publish } from "./index.js";

const VECTORS = new URL(
  "../../../../rust/crates/trinity-stack/vectors/",
  import.meta.url,
);

/** The crate's `vectors/cases.json`. */
export const cases = JSON.parse(
  await readFile(new URL("cases.json", VECTORS), "utf8"),
);

/** The crate's `vectors/refresh.json`, the reference reader's refresh cases. */
export const refreshCases = JSON.parse(
  await readFile(new URL("refresh.json", VECTORS), "utf8"),
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
