import { type FileStats, fileStats } from "../load-observer.js";
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
 *
 * The marker that completes a cache holds the files' {@link FileStats}, so a
 * reuse can say what it holds without reading it. A marker without them, from
 * an earlier build, gives `files: null`.
 */
export async function storeArchive(
  cid: string,
  origin: string,
  load: () => Promise<Map<string, Uint8Array>>,
  storage: { caches: CacheStorage; locks: Locks | undefined },
): Promise<{ reused: boolean; files: FileStats | null; storeMs?: number }> {
  if (storage.locks === undefined)
    throw new Error(
      "This browser has no Web Locks, which the product loader needs. Use a current browser on https or localhost.",
    );
  const name = archiveCacheName(cid);
  return storage.locks.request(name, async () => {
    if (await storage.caches.has(name)) {
      const existing = await storage.caches.open(name);
      const marker = await existing.match(completeKey(origin));
      if (marker !== undefined)
        return { reused: true, files: await readStats(marker) };
      await storage.caches.delete(name);
    }
    const files = await load();
    const writing = performance.now();
    const stats = fileStats(files);
    const cache = await storage.caches.open(name);
    for (const [path, bytes] of files)
      await cache.put(
        fileKey(origin, path),
        new Response(bytes as BodyInit, {
          headers: { "content-type": contentTypeOf(path) },
        }),
      );
    await cache.put(
      completeKey(origin),
      new Response(JSON.stringify(stats), {
        headers: { "content-type": "application/json" },
      }),
    );
    return {
      reused: false,
      files: stats,
      storeMs: performance.now() - writing,
    };
  });
}

async function readStats(marker: Response): Promise<FileStats | null> {
  try {
    const { count, bytes, largest } = (await marker.json()) as FileStats;
    if (!Number.isSafeInteger(count) || !Number.isSafeInteger(bytes))
      return null;
    return {
      count,
      bytes,
      ...(typeof largest?.path === "string" &&
      Number.isSafeInteger(largest.bytes)
        ? { largest: { path: largest.path, bytes: largest.bytes } }
        : {}),
    };
  } catch {
    return null;
  }
}
