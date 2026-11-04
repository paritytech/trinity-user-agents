import { describe, expect, test } from "bun:test";
import {
  chainBackendRows,
  chainBackendsNote,
  overridesFor,
  runningText,
} from "../../../src/app/chain-backends.js";
import {
  NETWORKS,
  chainBackends,
} from "../../../src/platform/network-config.js";

const PASEO = NETWORKS.paseo;
const DEFAULTS = chainBackends(PASEO);

describe("chain connection rows", () => {
  test("say what each chain runs on now when nothing waits for a reload", () => {
    const rows = chainBackendRows(PASEO, DEFAULTS, DEFAULTS);
    expect(rows.map(runningText)).toEqual([
      "Now: light client",
      "Now: RPC node wss://paseo-asset-hub-next-rpc.polkadot.io",
      "Now: RPC node wss://paseo-people-next-system-rpc.polkadot.io",
      "Now: RPC node wss://paseo-bulletin-next-rpc.polkadot.io",
    ]);
    expect(chainBackendsNote(rows)).toStartWith("Running as chosen.");
  });

  // A saved choice is not what runs until the tab reloads, and the menu
  // must not show it as the running connection.
  test("keep the running backend apart from a choice that waits for a reload", () => {
    const chosen = { ...DEFAULTS, people: "light-client" as const };
    const rows = chainBackendRows(PASEO, DEFAULTS, chosen);
    const people = rows.find((row) => row.role === "people")!;
    expect(people).toMatchObject({ running: "rpc", chosen: "light-client" });
    expect(runningText(people)).toBe(
      "Now: RPC node wss://paseo-people-next-system-rpc.polkadot.io · after reload: light client",
    );
    expect(chainBackendsNote(rows)).toStartWith("1 change waits for a reload.");
    const back = chainBackendRows(PASEO, chosen, DEFAULTS).find(
      (row) => row.role === "people",
    )!;
    expect(runningText(back)).toBe(
      "Now: light client · after reload: RPC node",
    );
  });
});

describe("saved overrides", () => {
  // Only what differs from the network's default is saved, so a later
  // change of a default reaches tabs that never chose for that chain.
  test("keep only the chains that differ from the default", () => {
    expect(overridesFor(PASEO, DEFAULTS)).toEqual({});
    expect(
      overridesFor(PASEO, {
        ...DEFAULTS,
        people: "light-client",
        relay: "rpc",
      }),
    ).toEqual({ relay: "rpc", people: "light-client" });
  });
});
