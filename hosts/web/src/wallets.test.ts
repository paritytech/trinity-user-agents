import { describe, expect, test } from "bun:test";
import { WalletStore } from "./wallets.js";
import { MemoryStorage } from "./test-support.js";

const KEY = "truapi-web-signing-host:wallets:v1";
const PHRASE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";

describe("WalletStore", () => {
  // Accounts are made and attested elsewhere, so a fresh host has nothing to
  // sign in with until one is imported.
  test("offers no wallet before one is imported", () => {
    expect(new WalletStore(new MemoryStorage()).list()).toEqual([]);
  });

  // `crypto.randomUUID` is undefined on plain http, where this host is also
  // opened from a phone, so ids must not be UUIDs.
  test("gives ids that need no secure context", () => {
    const store = new WalletStore(new MemoryStorage());
    expect(store.save("Plain http", PHRASE).id).toMatch(/^[0-9a-f]{32}$/);
  });

  // The wallet id is its storage namespace, so importing a phrase again must
  // land on the same state rather than an empty one.
  test("keeps one wallet per recovery phrase", () => {
    const store = new WalletStore(new MemoryStorage());
    const first = store.save("First", PHRASE);
    const again = store.save("Again", `  ${PHRASE.replaceAll(" ", "   ")}\n`);
    expect(again.id).toBe(first.id);
    expect(store.list()).toHaveLength(1);
  });

  test("derives the session entropy from the phrase", () => {
    const store = new WalletStore(new MemoryStorage());
    const wallet = store.save("Vector", PHRASE);
    expect(Buffer.from(wallet.entropy).toString("hex")).toBe(
      "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
    );
  });

  test("refuses a phrase that is not BIP-39", () => {
    const store = new WalletStore(new MemoryStorage());
    expect(() => store.save("Typo", "legal winner thank")).toThrow(
      "not a valid BIP-39",
    );
  });

  test("forgets a saved wallet and its phrase", () => {
    const storage = new MemoryStorage();
    const store = new WalletStore(storage);
    const wallet = store.save("Gone", PHRASE);
    store.forget(wallet.id);
    expect(store.find(wallet.id)).toBeUndefined();
    expect(store.has(wallet.id)).toBe(false);
    expect(storage.getItem(KEY)).not.toContain("legal winner");
  });
});

describe("WalletStore with damaged storage", () => {
  const OTHER_PHRASE =
    "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

  function stored(entries: unknown): MemoryStorage {
    const storage = new MemoryStorage();
    storage.setItem(
      KEY,
      typeof entries === "string" ? entries : JSON.stringify(entries),
    );
    return storage;
  }

  // One bad entry must not lock the developer out of the wallets that still work.
  test("lists the valid wallets and leaves out entries it cannot use", () => {
    const store = new WalletStore(
      stored([
        { id: "a", name: "Good", mnemonic: PHRASE },
        { id: "b", name: "Typo", mnemonic: "legal winner thank" },
        { id: "c", name: "No phrase" },
        "not an entry",
      ]),
    );
    expect(store.list().map((wallet) => wallet.id)).toEqual(["a"]);
    expect(store.problem()).toBe(
      `3 saved wallets are unreadable and left out. They stay in storage under "${KEY}".`,
    );
  });

  // The phrase is the only copy of a wallet, so a change made beside a bad
  // entry must not drop it.
  test("keeps unusable entries in storage when it saves or forgets", () => {
    const bad = { id: "b", name: "Typo", mnemonic: "legal winner thank" };
    const storage = stored([{ id: "a", name: "Good", mnemonic: PHRASE }, bad]);
    const store = new WalletStore(storage);
    store.save("New", OTHER_PHRASE);
    store.forget("a");
    const remaining = JSON.parse(storage.getItem(KEY) ?? "null") as {
      id: string;
    }[];
    expect(remaining.map((entry) => entry.id)).toEqual([
      "b",
      expect.any(String),
    ]);
    expect(remaining[0]).toEqual(bad);
  });

  test("reports nothing wrong for healthy or empty storage", () => {
    expect(new WalletStore(new MemoryStorage()).problem()).toBeNull();
    const store = new WalletStore(new MemoryStorage());
    store.save("Fine", PHRASE);
    expect(store.problem()).toBeNull();
  });

  // Overwriting text that fails to parse would destroy whatever a person could
  // still have recovered from it by hand.
  test("refuses changes over data that is not a wallet list, and leaves it untouched", () => {
    for (const raw of ['{"broken": ', '{"id":"a"}', "null", "7"]) {
      const storage = stored(raw);
      const store = new WalletStore(storage);
      expect(store.list()).toEqual([]);
      expect(store.problem()).toContain(KEY);
      expect(() => store.save("Fresh", PHRASE)).toThrow("Nothing was changed");
      expect(() => store.forget("a")).toThrow("Nothing was changed");
      expect(storage.getItem(KEY)).toBe(raw);
    }
  });

  // The message names where the data is, never what it holds.
  test("never puts a recovery phrase in its message", () => {
    const store = new WalletStore(
      stored([{ id: "b", name: "Typo", mnemonic: `${PHRASE} extra` }]),
    );
    expect(store.problem()).not.toContain("legal");
  });
});
