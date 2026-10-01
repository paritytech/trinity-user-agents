import { archiveCacheName, completeKey, fileKey } from "./files.js";
import { contentTypeOf } from "./serve.js";

/** The part of the Web Locks API the store needs. */
export type Locks = Pick<LockManager, "request">;

/**
 * Put an archive's verified files in the cache named by its content id.
 *
 * The cache is shared by every tab and wallet, which is safe because the id
 * fixes the bytes, and `load` checks them before they are written. A complete
 * cache is reused without loading. A partial one, left by a tab that closed
 * half way, is dropped and loaded again. One lock per id makes tabs take turns,
 * so two tabs opening the same content fetch it once and never write over each
 * other's half-finished cache.
 */
export async function storeArchive(
  cid: string,
  origin: string,
  load: () => Promise<Map<string, Uint8Array>>,
  storage: { caches: CacheStorage; locks: Locks | undefined },
): Promise<{ reused: boolean }> {
  if (storage.locks === undefined)
    throw new Error(
      "This browser has no Web Locks, which the product loader needs. Use a current browser on https or localhost.",
    );
  const name = archiveCacheName(cid);
  return storage.locks.request(name, async () => {
    if (await storage.caches.has(name)) {
      const existing = await storage.caches.open(name);
      if ((await existing.match(completeKey(origin))) !== undefined)
        return { reused: true };
      await storage.caches.delete(name);
    }
    const files = await load();
    const cache = await storage.caches.open(name);
    for (const [path, bytes] of files)
      await cache.put(
        fileKey(origin, path),
        new Response(bytes as BodyInit, {
          headers: { "content-type": contentTypeOf(path) },
        }),
      );
    await cache.put(completeKey(origin), new Response(""));
    return { reused: false };
  });
}
