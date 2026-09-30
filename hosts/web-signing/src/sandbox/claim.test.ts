import { describe, expect, test } from "bun:test";
import { OWNER_CACHE, OriginHeldError, claimOrigin } from "./claim.js";
import { noLocks, serialLocks } from "./test-support.js";

/** Cache Storage whose calls take a turn of the event loop, as the real one does. */
function slowCaches() {
  const stored = new Map<string, string>();
  const turn = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
  return {
    stored,
    caches: {
      open: async (name: string) => {
        expect(name).toBe(OWNER_CACHE);
        await turn();
        return {
          match: async (key: string) => {
            await turn();
            const body = stored.get(key);
            return body === undefined ? undefined : new Response(body);
          },
          put: async (key: string, response: Response) => {
            await turn();
            stored.set(key, await response.text());
          },
        } as unknown as Cache;
      },
    },
  };
}

const ORIGIN = "https://host.example:9450";

describe("claimOrigin", () => {
  test("records the first product and lets it back in", async () => {
    const { caches, stored } = slowCaches();
    const locks = serialLocks();
    await claimOrigin("a", ORIGIN, { caches, locks });
    await claimOrigin("a", ORIGIN, { caches, locks });
    expect(stored.get(`${ORIGIN}/__owner`)).toBe("a");
  });

  // Two tabs load two products into one origin at once. Both look, both find
  // it unowned, and without a lock both would write.
  test("lets one of two products that load at once have the origin", async () => {
    const { caches, stored } = slowCaches();
    const locks = serialLocks();
    const results = await Promise.allSettled([
      claimOrigin("a", ORIGIN, { caches, locks }),
      claimOrigin("b", ORIGIN, { caches, locks }),
    ]);
    expect(results.map((result) => result.status)).toEqual([
      "fulfilled",
      "rejected",
    ]);
    expect((results[1] as PromiseRejectedResult).reason).toBeInstanceOf(
      OriginHeldError,
    );
    expect(stored.get(`${ORIGIN}/__owner`)).toBe("a");
  });

  test("would let both have it without the lock", async () => {
    const { caches } = slowCaches();
    const results = await Promise.allSettled([
      claimOrigin("a", ORIGIN, { caches, locks: noLocks() }),
      claimOrigin("b", ORIGIN, { caches, locks: noLocks() }),
    ]);
    expect(results.map((result) => result.status)).toEqual([
      "fulfilled",
      "fulfilled",
    ]);
  });

  test("refuses a second product on a claimed origin and changes nothing", async () => {
    const { caches, stored } = slowCaches();
    const locks = serialLocks();
    await claimOrigin("a", ORIGIN, { caches, locks });
    await expect(claimOrigin("b", ORIGIN, { caches, locks })).rejects.toThrow(
      OriginHeldError,
    );
    expect(stored.get(`${ORIGIN}/__owner`)).toBe("a");
  });

  test("fails, and touches nothing, when the browser has no Web Locks", async () => {
    const { caches, stored } = slowCaches();
    await expect(
      claimOrigin("a", ORIGIN, { caches, locks: undefined }),
    ).rejects.toThrow("no Web Locks");
    expect(stored.size).toBe(0);
  });

  test("refuses an owner that is not a product label", async () => {
    const { caches, stored } = slowCaches();
    for (const owner of ["", "A", "a b", "-a", "x".repeat(64)])
      await expect(
        claimOrigin(owner, ORIGIN, { caches, locks: serialLocks() }),
      ).rejects.toThrow("which product");
    expect(stored.size).toBe(0);
  });
});
