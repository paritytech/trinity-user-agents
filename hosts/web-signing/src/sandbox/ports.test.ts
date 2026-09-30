import { describe, expect, test } from "bun:test";
import { PortLedger } from "./ports.js";
import { noLocks, serialLocks } from "./test-support.js";

function memory(initial: Record<string, string> = {}) {
  const saved = new Map(Object.entries(initial));
  return {
    saved,
    storage: {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => {
        saved.set(key, value);
      },
    },
  };
}

const OWNED = "truapi-web-signing-host:sandbox-ports";
const RETIRED = "truapi-web-signing-host:sandbox-ports-retired";

describe("PortLedger", () => {
  test("forgets nothing it was told and survives damaged storage", async () => {
    const { saved, storage } = memory({ [OWNED]: "not json" });
    const ports = new PortLedger(storage, serialLocks());
    expect(await ports.portFor("a", 9450, 9451)).toBe(9450);
    expect(JSON.parse(saved.get(OWNED) ?? "{}")).toEqual({ a: 9450 });
  });

  // The second tab starts while the first is between its read and its write.
  // Only a lock keeps it out of that window; the same run with no lock hands
  // both tabs one port.
  function racingTabs(locks: () => ReturnType<typeof serialLocks>) {
    const { saved, storage } = memory();
    const grants = locks();
    const other = new PortLedger(storage, grants);
    let started: Promise<number> | undefined;
    let armed = true;
    const tabOne = new PortLedger(
      {
        getItem: (key) => {
          if (key === RETIRED && armed) {
            armed = false;
            started = other.portFor("b", 9450, 9451);
          }
          return storage.getItem(key);
        },
        setItem: storage.setItem,
      },
      grants,
    );
    return {
      saved,
      run: async () => [await tabOne.portFor("a", 9450, 9451), await started],
    };
  }

  test("takes turns across tabs, so two products never get one port", async () => {
    const { saved, run } = racingTabs(serialLocks);
    expect(await run()).toEqual([9450, 9451]);
    expect(JSON.parse(saved.get(OWNED) ?? "{}")).toEqual({ a: 9450, b: 9451 });
  });

  test("would hand one port to both without the lock", async () => {
    const { run } = racingTabs(noLocks);
    expect(await run()).toEqual([9450, 9450]);
  });

  test("gives nothing when the browser has no Web Locks", async () => {
    const { saved, storage } = memory();
    const ports = new PortLedger(storage, null);
    await expect(ports.portFor("a", 9450, 9451)).rejects.toThrow(
      "no Web Locks",
    );
    await expect(ports.retire(9450)).rejects.toThrow("no Web Locks");
    expect(saved.size).toBe(0);
  });
});

describe("PortLedger retire", () => {
  // A port the ledger forgot can still hold a product's retained data. Retiring
  // it hands nobody that data and does not move a product that owns another port.
  test("skips a retired port and never gives it to anyone", async () => {
    const { storage } = memory();
    const ports = new PortLedger(storage, serialLocks());
    await ports.retire(9450);
    expect(await ports.portFor("a", 9450, 9452)).toBe(9451);
    expect(await ports.portFor("b", 9450, 9452)).toBe(9452);
    await expect(ports.portFor("c", 9450, 9452)).rejects.toThrow(
      "owned or retired",
    );
    expect(await ports.portFor("a", 9450, 9452)).toBe(9451);
  });

  // The loader refuses a port after the ledger already recorded it for the
  // product that was refused, so retiring must also release that mapping or the
  // product is handed the same port on every Open.
  test("releases the product that was mapped to the port it retires", async () => {
    const { storage } = memory();
    const ports = new PortLedger(storage, serialLocks());
    expect(await ports.portFor("b", 9450, 9452)).toBe(9450);
    expect(await ports.portFor("a", 9450, 9452)).toBe(9451);
    await ports.retire(9450);
    expect(await ports.portFor("b", 9450, 9452)).toBe(9452);
    expect(await ports.portFor("a", 9450, 9452)).toBe(9451);
  });

  // A write that stopped between the two keys leaves a port that is retired and
  // still mapped. The product must not be handed it again.
  test("does not hand a product a port that is retired while still mapped", async () => {
    const { storage } = memory({
      [OWNED]: JSON.stringify({ a: 9450 }),
      [RETIRED]: JSON.stringify([9450]),
    });
    const ports = new PortLedger(storage, serialLocks());
    expect(await ports.portFor("a", 9450, 9452)).toBe(9451);
  });

  // Retiring and allocating from different tabs at once must not lose either.
  test("keeps both a retirement and an allocation made at the same time", async () => {
    const { saved, storage } = memory();
    const tabOne = new PortLedger(storage, serialLocks());
    const grants = serialLocks();
    const [one, two] = [
      new PortLedger(storage, grants),
      new PortLedger(storage, grants),
    ];
    await tabOne.portFor("a", 9450, 9455);
    await Promise.all([one.retire(9450), two.portFor("b", 9450, 9455)]);
    expect(JSON.parse(saved.get(RETIRED) ?? "[]")).toEqual([9450]);
    expect(JSON.parse(saved.get(OWNED) ?? "{}")).toEqual({ b: 9451 });
  });

  test("keeps the retired set across ledgers on one storage and survives damaged data", async () => {
    const { storage } = memory({ [RETIRED]: "not json" });
    await new PortLedger(storage, serialLocks()).retire(9451);
    expect(
      await new PortLedger(storage, serialLocks()).portFor("a", 9450, 9452),
    ).toBe(9450);
    expect(
      await new PortLedger(storage, serialLocks()).portFor("b", 9450, 9452),
    ).toBe(9452);
  });
});
