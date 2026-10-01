import { describe, expect, test } from "bun:test";
import { archiveCacheName, completeKey } from "./files.js";
import { type Locks, storeArchive } from "./store.js";

const ORIGIN = "https://host.example";

/** An in-memory CacheStorage: a named map of keys to bodies. */
function fakeCaches(): CacheStorage & { names(): string[] } {
  const stores = new Map<string, Map<string, string>>();
  const cache = (name: string) => {
    const entries = stores.get(name) ?? new Map<string, string>();
    stores.set(name, entries);
    return {
      match: async (key: string) =>
        entries.has(key) ? new Response(entries.get(key)) : undefined,
      put: async (key: string, response: Response) => {
        entries.set(key, await response.text());
      },
    };
  };
  return {
    has: async (name: string) => stores.has(name),
    open: async (name: string) => cache(name),
    delete: async (name: string) => stores.delete(name),
    names: () => [...stores.keys()],
  } as unknown as CacheStorage & { names(): string[] };
}

/** Grants each name to one holder at a time, in request order, as Web Locks do. */
function serialLocks(): Locks {
  const tails = new Map<string, Promise<unknown>>();
  return {
    request: ((name: string, callback: () => unknown) => {
      const run = (tails.get(name) ?? Promise.resolve()).then(callback);
      tails.set(
        name,
        run.catch(() => undefined),
      );
      return run;
    }) as Locks["request"],
  };
}

const files = () => new Map([["index.html", new Uint8Array([1])]]);

describe("storeArchive", () => {
  // Two tabs opening the same content, for two wallets, must not each fetch
  // it or write into each other's cache.
  test("loads once for tabs opening the same content together", async () => {
    const caches = fakeCaches();
    const locks = serialLocks();
    let loads = 0;
    const load = async () => {
      loads += 1;
      await Promise.resolve();
      return files();
    };
    const results = await Promise.all([
      storeArchive("bafy-a", ORIGIN, load, { caches, locks }),
      storeArchive("bafy-a", ORIGIN, load, { caches, locks }),
    ]);
    expect(loads).toBe(1);
    expect(results.map((result) => result.reused)).toEqual([false, true]);
  });

  test("keeps each content in a cache of its own", async () => {
    const caches = fakeCaches();
    const locks = serialLocks();
    await storeArchive("bafy-a", ORIGIN, async () => files(), {
      caches,
      locks,
    });
    await storeArchive("bafy-b", ORIGIN, async () => files(), {
      caches,
      locks,
    });
    expect(caches.names().sort()).toEqual(
      [archiveCacheName("bafy-a"), archiveCacheName("bafy-b")].sort(),
    );
  });

  test("drops a cache a closed tab left half written, and loads again", async () => {
    const caches = fakeCaches();
    const locks = serialLocks();
    const partial = await caches.open(archiveCacheName("bafy-a"));
    await partial.put(`${ORIGIN}/__files/index.html`, new Response("stale"));
    const result = await storeArchive("bafy-a", ORIGIN, async () => files(), {
      caches,
      locks,
    });
    expect(result.reused).toBe(false);
    const stored = await caches.open(archiveCacheName("bafy-a"));
    expect(await (await stored.match(completeKey(ORIGIN)))?.text()).toBe("");
  });

  test("leaves no complete marker when loading fails", async () => {
    const caches = fakeCaches();
    const locks = serialLocks();
    await expect(
      storeArchive(
        "bafy-a",
        ORIGIN,
        async () => {
          throw new Error("gateway down");
        },
        { caches, locks },
      ),
    ).rejects.toThrow("gateway down");
    expect(caches.names()).toEqual([]);
  });

  test("refuses to run without Web Locks", async () => {
    await expect(
      storeArchive("bafy-a", ORIGIN, async () => files(), {
        caches: fakeCaches(),
        locks: undefined,
      }),
    ).rejects.toThrow("Web Locks");
  });
});
