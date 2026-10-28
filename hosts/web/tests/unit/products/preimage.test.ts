import { describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import type { FetchLike } from "../../../src/archive/gateway.js";
import { gatewayPreimages, preimageCid } from "../../../src/preimage.js";

const GATEWAY = "https://gateway.example";
const ICON = new TextEncoder().encode("synthetic icon bytes");
const KEY = blake2b(ICON, { dkLen: 32 });
/** Computed outside this code, from the CIDv1 raw blake2b-256 layout Bulletin serves. */
const ICON_CID =
  "bafk2bzaced3rmax523cjwnedn4lcptflflxt35nzkz7ruv2rxobgsyfhult4q";

async function firstItem(key: Uint8Array, fetchFn: FetchLike) {
  const iterator = gatewayPreimages(GATEWAY, fetchFn)
    .lookupPreimage(key)
    [Symbol.asyncIterator]();
  const { value } = await iterator.next();
  await iterator.return?.();
  return value?._unsafeUnwrap();
}

describe("preimage lookups", () => {
  test("name the content by the CID Bulletin serves it under", () => {
    expect(preimageCid(KEY)).toBe(ICON_CID);
    expect(() => preimageCid(KEY.subarray(1))).toThrow();
  });

  test("read the bytes from the gateway's path for that CID", async () => {
    const asked: string[] = [];
    const value = await firstItem(KEY, async (url: string) => {
      asked.push(url);
      return new Response(ICON);
    });
    expect({ asked, value: [...(value ?? [])] }).toEqual({
      asked: [`${GATEWAY}/ipfs/${ICON_CID}`],
      value: [...ICON],
    });
  });

  // A product waits on a miss for a later value, so a gateway without the
  // content, or none at all, must answer a miss rather than end the lookup.
  test("give a miss when the gateway lacks the content or cannot be reached", async () => {
    const missing = await firstItem(
      KEY,
      async () => new Response("no", { status: 404 }),
    );
    const unreachable = await firstItem(KEY, async () => {
      throw new TypeError("network down");
    });
    expect([missing, unreachable]).toEqual([undefined, undefined]);
  });

  test("stop the fetch when the core cancels the lookup", async () => {
    let signal: AbortSignal | undefined;
    const iterator = gatewayPreimages(GATEWAY, (_url, init) => {
      signal = init?.signal ?? undefined;
      return new Promise<Response>(() => {});
    })
      .lookupPreimage(KEY)
      [Symbol.asyncIterator]();
    await iterator.return?.();
    expect(signal?.aborted).toBe(true);
  });
});
