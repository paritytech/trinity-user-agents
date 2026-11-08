import { afterEach, describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { equalBytes } from "../../../src/platform/codec.js";
import { blake2b256 } from "../../../src/platform/crypto.js";
import { decodeStatement } from "../../../src/platform/statement.js";
import { productSlotPrefix } from "../../../src/platform/storage.js";
import {
  HostBackup,
  MAX_WRITES_PER_DAY,
  MIN_INTERVAL_MS,
  QUIET_MS,
  type BackupSession,
} from "../../../src/backup/host-backup.js";
import { POINTER_MAGIC } from "../../../src/backup/sealing.js";
import { MemoryStatementStore } from "../../support/statement-memory.js";
import { MemoryStorage } from "../../support/test-support.js";

const GENESIS = `0x${"aa".repeat(32)}` as HexString;
const IDENTITY = `0x${"1d".repeat(32)}` as HexString;
const ENTROPY = new Uint8Array(16).fill(0xab);
const BACKUP_SECRET = sr25519.secretFromSeed(new Uint8Array(32).fill(0xb4));
const BACKUP_ACCOUNT = sr25519.getPublicKey(BACKUP_SECRET);

const session = (
  walletId: string,
  overrides: Partial<BackupSession> = {},
): BackupSession => ({
  network: "paseo",
  walletId,
  entropy: ENTROPY,
  peopleGenesis: GENESIS,
  identity: IDENTITY,
  ...overrides,
});
const record = (walletId: string, kind: string) =>
  `truapi-web-signing-host:network:paseo:${kind}:${walletId}:v1`;
const product = (walletId: string, key: string) =>
  `${productSlotPrefix(walletId, "paseo")}truapi:product-storage:v1:${key}`;

/** One network both browsers reach: a Statement Store, Bulletin, and what was claimed and stored on it. */
class Network {
  readonly store = new MemoryStatementStore();
  readonly bulletin = new Map<string, Uint8Array>();
  claims = 0;
  stores = 0;
  /** Bytes the gateway serves instead of what was stored, when set. */
  tampered: Uint8Array | null = null;

  pointers(): Uint8Array[] {
    return this.store.statements;
  }
}

const started: HostBackup[] = [];
afterEach(() => started.splice(0).forEach((backup) => backup.stop()));

/** A browser's backup on `network`, on a clock the test moves. */
function browser(network: Network, storage = new MemoryStorage()) {
  const clock = { at: Date.UTC(2026, 10, 8, 12) };
  const lines: string[] = [];
  const backup = new HostBackup({
    storage,
    core: {
      account: async () => BACKUP_ACCOUNT,
      claimSlot: async (): Promise<StatementAccountClaim> => {
        network.claims += 1;
        return {
          account: bytesToHex(BACKUP_ACCOUNT),
          period: Math.floor(clock.at / 86_400_000),
          outcome: network.claims === 1 ? "registered" : "alreadyAllocated",
          seq: 0,
          collection: "LitePeople",
        };
      },
      // The core signs pointers only.
      signPointer: async (payload) => {
        const magic = bytesToHex(POINTER_MAGIC).slice(2);
        if (!bytesToHex(payload).includes(magic))
          throw new Error("not a backup pointer");
        return sr25519.sign(BACKUP_SECRET, payload);
      },
      storeSnapshot: async (sealed) => {
        network.stores += 1;
        const key = blake2b256(sealed);
        network.bulletin.set(bytesToHex(key), sealed);
        return key;
      },
    },
    openStore: async () => network.store.connect(),
    fetchSnapshot: async (key) =>
      network.tampered ?? network.bulletin.get(bytesToHex(key)),
    settled: async () => {},
    acquire: async () => () => {},
    log: (line) => lines.push(line),
    now: () => clock.at,
    readMs: 30,
  });
  started.push(backup);
  /** Look at the state now, then again once it held still. */
  const settle = async (who: BackupSession) => {
    await backup.check(who);
    clock.at += QUIET_MS;
    await backup.check(who);
  };
  return { backup, storage, clock, lines, settle };
}

/** Browser A, signed in with the wallet, with chat state, and product state a product sealed itself. */
function browserA(network: Network) {
  const a = browser(network);
  a.storage.setItem(record("wa", "notifications"), '[{"id":"n1"}]');
  a.storage.setItem(record("wa", "nova-chat"), '{"outgoing":["request-1"]}');
  a.storage.setItem(
    product("wa", "17:kettle-chat.paseo:kettle-chat:node:0011:state"),
    "0x5ea1ed",
  );
  a.storage.setItem("truapi-web-signing-host:wallets:v1", "[phrase]");
  return a;
}

describe("a backup kept in the background", () => {
  test("is stored once the state held still, and pointed to by the backup account, never the identity", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);

    await a.backup.check(wallet);
    expect(network.stores).toBe(0);
    a.clock.at += QUIET_MS;
    await a.backup.check(wallet);

    expect(network.stores).toBe(1);
    expect(network.claims).toBe(1);
    const [pointer] = network.pointers().map(decodeStatement);
    expect(network.pointers()).toHaveLength(1);
    expect(equalBytes(pointer!.signer, BACKUP_ACCOUNT)).toBe(true);
    // The snapshot is sealed: nothing of the state reads in the clear.
    const sealed = [...network.bulletin.values()][0]!;
    expect(new TextDecoder().decode(sealed)).not.toContain("request-1");
    expect(new TextDecoder().decode(sealed)).not.toContain("phrase");
  });

  test("stores nothing while the state keeps changing, nor again before the shortest interval", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    for (let change = 0; change < 3; change++) {
      a.storage.setItem(record("wa", "chat"), `[contact ${change}]`);
      await a.backup.check(wallet);
      a.clock.at += QUIET_MS;
    }
    expect(network.stores).toBe(0);

    await a.settle(wallet);
    expect(network.stores).toBe(1);
    await a.settle(wallet);
    expect(network.stores).toBe(1);

    a.storage.setItem(record("wa", "chat"), "[a new contact]");
    await a.settle(wallet);
    expect(network.stores).toBe(1);
    a.clock.at += MIN_INTERVAL_MS;
    await a.settle(wallet);
    expect(network.stores).toBe(2);
    // The second pointer replaced the first on its channel.
    expect(network.pointers()).toHaveLength(1);
    expect(network.store.submitted).toHaveLength(2);
    const [first, second] = network.store.submitted.map(decodeStatement);
    expect(second!.expiry > first!.expiry).toBe(true);
  });

  test("stores at most its daily limit, failed tries included", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    network.tampered = new Uint8Array([1]);
    for (let look = 0; look < 40; look++) {
      a.storage.setItem(record("wa", "chat"), `[${look}]`);
      await a.settle(wallet);
      a.clock.at += 2 * 60 * 60_000;
      if (a.clock.at >= Date.UTC(2026, 10, 9)) break;
    }
    expect(network.stores).toBe(MAX_WRITES_PER_DAY);
    expect(network.pointers()).toHaveLength(0);
    expect(a.lines.some((line) => line.includes("other bytes"))).toBe(true);
  });
});

describe("the pointer, kept alive", () => {
  test("its slot is claimed once a day, and the pointer is posted again when the store lost it, with no new snapshot", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);
    expect([network.stores, network.claims]).toEqual([1, 1]);

    await a.backup.check(wallet);
    expect(network.claims).toBe(1);

    network.store.statements = [];
    a.clock.at += 86_400_000;
    await a.backup.check(wallet);
    expect([network.stores, network.claims]).toEqual([1, 2]);
    const pointers = network.pointers().map(decodeStatement);
    expect(pointers).toHaveLength(1);

    // A fresh browser finds the snapshot through the pointer posted again.
    expect(await browser(network).backup.prepare(session("wb"))).toBe(
      "restored",
    );
  });
});

describe("a browser that signs in with the same wallet", () => {
  test("restores the backup when it holds nothing of the wallet, under its own wallet id, claiming and storing nothing", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);
    const claims = network.claims;

    const b = browser(network);
    expect(await b.backup.prepare(session("wb"))).toBe("restored");

    expect(b.storage.getItem(record("wb", "nova-chat"))).toBe(
      '{"outgoing":["request-1"]}',
    );
    expect(
      b.storage.getItem(
        product("wb", "17:kettle-chat.paseo:kettle-chat:node:0011:state"),
      ),
    ).toBe("0x5ea1ed");
    expect(b.storage.getItem("truapi-web-signing-host:wallets:v1")).toBeNull();
    expect(network.claims).toBe(claims);
    expect(network.stores).toBe(1);

    // Its own writer finds the same state: it stores, claims and posts
    // nothing, and leaves the pointer to the browser that published it.
    const restored = session("wb");
    b.backup.start(restored);
    await b.settle(restored);
    b.clock.at += 86_400_000;
    await b.settle(restored);
    expect(network.stores).toBe(1);
    expect(network.claims).toBe(claims);
    expect(network.store.submitted).toHaveLength(1);
  });

  test("restores even when which revision it was cannot be kept", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);

    const b = browser(network);
    const setItem = b.storage.setItem.bind(b.storage);
    b.storage.setItem = (key, value) => {
      if (key.includes(":backup:")) throw new Error("QuotaExceededError");
      setItem(key, value);
    };
    expect(await b.backup.prepare(session("wb"))).toBe("restored");
    expect(b.storage.getItem(record("wb", "nova-chat"))).not.toBeNull();
  });

  test("restores nothing, and keeps no backup, in a browser without locks", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);

    const lockless = new HostBackup({
      storage: new MemoryStorage(),
      core: {
        account: async () => BACKUP_ACCOUNT,
        claimSlot: async () => {
          throw new Error("no claim expected");
        },
        signPointer: async () => {
          throw new Error("no signature expected");
        },
        storeSnapshot: async () => {
          throw new Error("no store expected");
        },
      },
      openStore: async () => network.store.connect(),
      fetchSnapshot: async () => undefined,
      settled: async () => {},
      acquire: async () => {
        throw new Error("this browser has no Web Locks");
      },
      log: () => {},
      readMs: 30,
    });
    expect(await lockless.prepare(session("wb"))).toBe("failed");
  });

  test("keeps the state it already holds, and only says another browser published", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);

    const c = browser(network);
    c.storage.setItem(record("wc", "chat"), "[its own]");
    expect(await c.backup.prepare(session("wc"))).toBe("kept");
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(c.storage.getItem(record("wc", "nova-chat"))).toBeNull();
    expect(c.storage.getItem(record("wc", "chat"))).toBe("[its own]");
    expect(
      c.lines.some((line) => line.includes("another browser published")),
    ).toBe(true);
  });

  test("finds nothing of another identity's or another network's backup", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);

    const other = browser(network);
    expect(
      await other.backup.prepare(
        session("wb", { identity: `0x${"2e".repeat(32)}` as HexString }),
      ),
    ).toBe("none");
    expect(
      await other.backup.prepare(
        session("wb", { peopleGenesis: `0x${"bb".repeat(32)}` as HexString }),
      ),
    ).toBe("none");
    expect(other.storage.keys()).toEqual([]);
  });

  test("restores nothing from bytes that are not the snapshot, or for a session that ended", async () => {
    const network = new Network();
    const a = browserA(network);
    const wallet = session("wa");
    a.backup.start(wallet);
    await a.settle(wallet);

    const b = browser(network);
    network.tampered = new Uint8Array(64);
    expect(await b.backup.prepare(session("wb"))).toBe("failed");
    network.tampered = null;
    expect(await b.backup.prepare(session("wb"), () => false)).toBe("failed");
    expect(b.storage.keys()).toEqual([]);
  });
});
