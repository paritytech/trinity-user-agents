import { describe, expect, test } from "bun:test";
import type { NetworkConfig } from "../../../src/platform/network-config.js";
import {
  ProductWorkers,
  workersKey,
} from "../../../src/products/product-workers.js";
import { MemoryStorage } from "../../support/test-support.js";

/** Workers with no core: nothing starts, so only the list is exercised. */
function listOnly(defaults: string[]) {
  const storage = new MemoryStorage();
  const workers = new ProductWorkers({
    runtime: () => null,
    resolve: async () => {
      throw new Error("not resolved in this test");
    },
    container: {} as HTMLElement,
    base: "/",
    storage,
    tap: () => undefined,
    log: () => {},
    defaults: (network) => (network.id === "paseo" ? defaults : []),
  });
  return { workers, storage };
}

const network = (id: string) => ({ id }) as NetworkConfig;

describe("product workers", () => {
  test("lists the host's default workers first, for the network they are for, and never removes them", async () => {
    const { workers, storage } = listOnly(["placed.paseo"]);
    expect(workers.names()).toEqual([]);
    workers.setSession({ walletId: "w1", network: network("paseo") });
    expect(workers.names()).toEqual(["placed.paseo"]);
    expect(workers.isDefault("placed.paseo")).toBe(true);

    await workers.add("Mine.paseo");
    expect(workers.names()).toEqual(["placed.paseo", "mine.paseo"]);
    expect(workers.isDefault("mine.paseo")).toBe(false);
    // Only what the user added is saved; the defaults come from the host each time.
    expect(JSON.parse(storage.getItem(workersKey("paseo", "w1"))!)).toEqual([
      "mine.paseo",
    ]);

    workers.remove("placed.paseo");
    workers.remove("mine.paseo");
    expect(workers.names()).toEqual(["placed.paseo"]);

    workers.setSession({ walletId: "w1", network: network("other") });
    expect(workers.names()).toEqual([]);
  });
});
