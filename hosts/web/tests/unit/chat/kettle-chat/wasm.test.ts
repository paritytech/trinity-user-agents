/**
 * Kettle's engine as `npm run build:kettle` builds it (`kettle-chat-wasm/`),
 * run here over scripted ports. Skipped until it is built.
 */
import { afterEach, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { blake2b256 } from "../../../../src/platform/crypto.js";
import {
  KETTLE_GLUE,
  KETTLE_WASM,
  KETTLE_WASM_PACKAGE,
} from "../../../../src/chat/kettle-chat/route.js";
import type {
  KettleModule,
  KettlePorts,
} from "../../../../src/chat/kettle-chat/ports.js";

const pkg = new URL(`../../../../${KETTLE_WASM_PACKAGE}/`, import.meta.url);
const built = existsSync(new URL(KETTLE_WASM, pkg));

const ACCOUNT = `0x${"11".repeat(32)}`;
const hex = (byte: string, length = 32) => `0x${byte.repeat(length)}`;

let kettle: KettleModule;
let irohRelays: (options: unknown) => string[];
let irohAllows: (options: unknown, relays: string[]) => boolean;

beforeAll(async () => {
  if (!built) return;
  const glue = await import(new URL(KETTLE_GLUE, pkg).href);
  await glue.default({
    module_or_path: readFileSync(new URL(KETTLE_WASM, pkg)),
  });
  irohRelays = (options) => glue.kettleIrohRelays(options);
  irohAllows = (options, relays) => glue.kettleIrohAllows(options, relays);
  kettle = {
    revision: glue.kettleRevision(),
    protocol: glue.kettleProtocol(),
    start: (account, ports, onEvent, options) =>
      glue.KettleNode.start(account, ports, onEvent, options),
    removeState: (account, ports) =>
      glue.KettleNode.removeState(account, ports),
  };
});

/** Ports over a map, with every subscription's `deliver` kept for the test. */
function scripted() {
  const storage = new Map<string, Uint8Array>();
  const delivers: ((statement: unknown) => void)[] = [];
  const ports: KettlePorts = {
    submit: async () => ({ status: "new" }),
    subscribe: (_filter, deliver) => {
      delivers.push(deliver);
      return () => {};
    },
    entropy: async (context) => blake2b256(context),
    requestAllowance: async () => "allocated",
    read: async (name) => storage.get(name),
    write: async (name, value) => void storage.set(name, value),
    remove: async (name) => void storage.delete(name),
  };
  return { ports, storage, delivers };
}

const panics: string[] = [];
const consoleError = console.error;
afterEach(() => {
  console.error = consoleError;
  panics.length = 0;
});

describe("Kettle's engine in this host's WebAssembly", () => {
  test.skipIf(!built)(
    "a statement with more topics than the store allows is dropped, and the node keeps running",
    async () => {
      console.error = (...args: unknown[]) => {
        panics.push(args.map(String).join(" "));
      };
      const { ports, delivers } = scripted();
      const node = await kettle.start(ACCOUNT, ports, () => {}, {
        fast: true,
      });
      for (let wait = 0; delivers.length === 0 && wait < 50; wait += 1)
        await new Promise((resolve) => setTimeout(resolve, 20));
      expect(delivers.length).toBeGreaterThan(0);
      const statement = {
        expiry: 1n << 40n,
        topics: ["01", "02", "03", "04", "05"].map((byte) => hex(byte)),
        data: "0x00",
        proof: {
          tag: "Sr25519",
          value: { signature: hex("aa", 64), signer: hex("bb") },
        },
      };
      for (const deliver of delivers) deliver(statement);
      await new Promise((resolve) => setTimeout(resolve, 200));
      // A module that panicked would throw on any call after.
      expect(await node.directGroup(hex("22"))).toBeNull();
      await node.stop();
      node.free();
      expect(panics).toEqual([]);
    },
  );

  test.skipIf(!built)(
    "removeState removes the sealed state a stopped node of the account kept, and nothing else",
    async () => {
      const { ports, storage } = scripted();
      storage.set("unrelated", new Uint8Array([1]));
      const node = await kettle.start(ACCOUNT, ports, () => {}, {
        fast: true,
      });
      await node.saveState();
      await node.stop();
      node.free();
      const state = [...storage.keys()].filter((key) => key !== "unrelated");
      expect(state).toHaveLength(1);
      await kettle.removeState(ACCOUNT, ports);
      expect([...storage.keys()]).toEqual(["unrelated"]);
    },
  );

  test.skipIf(!built)(
    "without a relay a node runs no iroh endpoint; with a relay it does not reach, it says why within its timeout and runs on the Statement Store alone",
    async () => {
      const off = await kettle.start(ACCOUNT, scripted().ports, () => {}, {
        fast: true,
      });
      const offStatus = off.iroh();
      await off.stop();
      off.free();
      const started = Date.now();
      const unreachable = await kettle.start(
        hex("33"),
        scripted().ports,
        () => {},
        {
          fast: true,
          iroh: {
            relays: "custom",
            relayUrl: "http://127.0.0.1:9/",
            onlineTimeoutMs: 500,
          },
        },
      );
      const waitedMs = Date.now() - started;
      const status = unreachable.iroh();
      // It runs: a module that failed would throw here.
      expect(await unreachable.directGroup(hex("22"))).toBeNull();
      await unreachable.stop();
      unreachable.free();
      expect([offStatus, status]).toEqual([
        { state: "off" },
        {
          state: "unavailable",
          relays: "custom",
          relay: "http://127.0.0.1:9/",
          reason: "the endpoint reached no relay in 500 ms",
        },
      ]);
      expect(waitedMs).toBeLessThan(5_000);
    },
  );

  test.skipIf(!built)(
    "a relay URL with a user, password, path, query or fragment binds no endpoint, and the node's status does not repeat it",
    async () => {
      // A synthetic stand-in for a secret.
      const secret = "synthetic-secret-2";
      const statuses = [];
      for (const relayUrl of [
        `http://user:${secret}@127.0.0.1:9`,
        `http://127.0.0.1:9/?token=${secret}`,
        `http://127.0.0.1:9/#${secret}`,
        `http://127.0.0.1:9/${secret}`,
      ]) {
        const node = await kettle.start(hex("44"), scripted().ports, () => {}, {
          fast: true,
          iroh: { relays: "custom", relayUrl, onlineTimeoutMs: 500 },
        });
        statuses.push(node.iroh());
        await node.stop();
        node.free();
      }
      expect(JSON.stringify(statuses).includes(secret)).toBe(false);
      expect(statuses).toEqual(
        Array(4).fill({
          state: "unavailable",
          reason:
            "the relay must be an http or https URL with a host, and no user, password, path, query or fragment",
        }),
      );
    },
  );

  test.skipIf(!built)(
    "the public test relays are the four iroh 1.4.1 ships as its default; a custom relay is itself; off is none; an iroh request that does not read binds nothing and says why",
    async () => {
      const bad = await kettle.start(hex("55"), scripted().ports, () => {}, {
        fast: true,
        iroh: { relays: "private" } as never,
      });
      const badStatus = bad.iroh();
      await bad.stop();
      bad.free();
      expect([
        irohRelays({ iroh: { relays: "public" } }),
        irohRelays({
          iroh: { relays: "custom", relayUrl: "http://127.0.0.1:3340" },
        }),
        irohRelays({}),
        badStatus,
      ]).toEqual([
        [
          "https://aps1-1.relay.n0.iroh.link./",
          "https://euc1-1.relay.n0.iroh.link./",
          "https://use1-1.relay.n0.iroh.link./",
          "https://usw1-1.relay.n0.iroh.link./",
        ],
        ["http://127.0.0.1:3340/"],
        [],
        {
          state: "unavailable",
          reason:
            'options.iroh must be { relays: "public" } or { relays: "custom", relayUrl }',
        },
      ]);
      expect(() =>
        irohRelays({
          iroh: {
            relays: "custom",
            relayUrl: "https://u:synthetic-4@x.example",
          },
        }),
      ).toThrow(/^the relay must be/);
    },
  );

  test.skipIf(!built)(
    "an endpoint dials a peer only when every relay its address names is one of the endpoint's own: any public test relay, or the one custom relay",
    () => {
      const publicRelays = { iroh: { relays: "public" } };
      const custom = {
        iroh: { relays: "custom", relayUrl: "http://127.0.0.1:3340" },
      };
      const cases = [
        ["https://euc1-1.relay.n0.iroh.link./"],
        ["https://usw1-1.relay.n0.iroh.link./"],
        // The same host written without its final dot.
        ["https://euc1-1.relay.n0.iroh.link"],
        ["https://euc1-1.relay.n0.iroh.link./", "https://relay.example"],
        ["https://relay.example"],
        ["https://euc1-1.relay.n0.iroh.link:8443"],
        ["https://euc1-1.relay.n0.iroh.link/?token=x"],
        ["http://euc1-1.relay.n0.iroh.link"],
        ["https://euc1-1.staging-relay.n0.iroh.link./"],
        ["http://127.0.0.1:3340"],
        ["http://127.0.0.1:3341"],
        ["http://localhost:3340"],
        [],
      ];
      expect(
        cases.map((relays) => [
          irohAllows(publicRelays, relays),
          irohAllows(custom, relays),
          irohAllows({}, relays),
        ]),
      ).toEqual([
        [true, false, false],
        [true, false, false],
        [true, false, false],
        [false, false, false],
        [false, false, false],
        [false, false, false],
        [false, false, false],
        [false, false, false],
        [false, false, false],
        [false, true, false],
        [false, false, false],
        [false, false, false],
        [false, false, false],
      ]);
    },
  );
});
