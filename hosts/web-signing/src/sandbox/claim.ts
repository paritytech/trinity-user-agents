import type { Locks } from "./ports.js";

export const OWNER_CACHE = "truapi-sandbox-owner";
const OWNER_LOCK = "truapi-sandbox-owner";
const OWNER_LABEL = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/;

/** This origin already belongs to another product. */
export class OriginHeldError extends Error {}

/**
 * Record `owner` on `origin` at first use, and refuse any other product
 * afterwards. The host's port ledger lives in one browser and can be lost, so
 * a port it hands out again may still hold another product's retained data.
 * The record is kept in Cache Storage, which the container removes from the
 * product, so an enforced product cannot rewrite it.
 *
 * Looking and writing run under one Web Lock, so two loaders on the origin
 * cannot both find it unowned. With no Web Locks the claim fails.
 */
export async function claimOrigin(
  owner: string,
  origin: string,
  storage: {
    caches: Pick<CacheStorage, "open">;
    locks: Locks | undefined;
  },
): Promise<void> {
  if (owner.length > 63 || !OWNER_LABEL.test(owner))
    throw new Error("The sandbox was not told which product this is.");
  if (storage.locks === undefined)
    throw new Error(
      "This origin cannot be claimed safely: this browser has no Web Locks.",
    );
  await storage.locks.request(OWNER_LOCK, async () => {
    const cache = await storage.caches.open(OWNER_CACHE);
    const key = `${origin}/__owner`;
    const held = await cache.match(key);
    if (held === undefined) {
      await cache.put(key, new Response(owner));
      return;
    }
    if ((await held.text()) !== owner)
      throw new OriginHeldError(
        "This origin holds another product's data. Nothing was changed.",
      );
  });
}
