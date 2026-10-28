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
  const sessions: number[] = [];
  const workers = new ProductWorkers({
    runtime: () => null,
    resolve: async () => {
      throw new Error("not resolved in this test");
    },
    container: {} as HTMLElement,
    base: "/",
    storage,
    tap: () => undefined,
    activity: () => {},
    defaults: (network) => (network.id === "paseo" ? defaults : []),
    observer: {
      sessionChanged: () => sessions.push(sessions.length + 1),
      console: () => {},
    },
  });
  return { workers, storage, sessions };
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

  // The inspector shows what the manager holds, not what a console implies:
  // a worker that has not started says so, and nothing it does not own can
  // be restarted from there.
  test("lists every worker the session wants with its own state, and offers no restart for one that has not started", async () => {
    const { workers } = listOnly(["placed.paseo"]);
    workers.setSession({ walletId: "w1", network: network("paseo") });
    await workers.add("mine.paseo");
    await workers.holdForPocket("card.paseo", ["humanity"]);
    expect(workers.inspect()).toEqual([
      {
        name: "placed.paseo",
        run: null,
        state: null,
        since: null,
        surfaces: ["chat"],
        addedByHost: true,
        refusals: [],
        restartable: false,
      },
      {
        name: "mine.paseo",
        run: null,
        state: null,
        since: null,
        surfaces: ["chat"],
        addedByHost: false,
        refusals: [],
        restartable: false,
      },
      {
        name: "card.paseo",
        run: null,
        state: null,
        since: null,
        surfaces: ["pocket"],
        addedByHost: false,
        refusals: [],
        restartable: false,
      },
    ]);
  });

  // What the inspector saw of one session's workers must not show in the next.
  test("tells the observer when the session changes, and only then", () => {
    const { workers, sessions } = listOnly([]);
    workers.setSession({ walletId: "w1", network: network("paseo") });
    workers.setSession({ walletId: "w1", network: network("paseo") });
    workers.setSession({ walletId: "w2", network: network("paseo") });
    workers.setSession(null);
    expect(sessions.length).toBe(3);
  });
});
