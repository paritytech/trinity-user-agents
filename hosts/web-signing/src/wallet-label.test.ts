import { describe, expect, test } from "bun:test";
import {
  WalletPublicInfoStore,
  shortKey,
  usernameFrom,
  walletLabel,
} from "./wallet-label.js";
import { MemoryStorage } from "./test-support.js";

const KEY = `0x${"ab".repeat(32)}`;
const wallet = (name: string, id = "1234abcd") => ({ id, name });

describe("wallet label", () => {
  // Generic names are what made the picker useless, so the tail must differ.
  test("tells wallets with the same generic name apart before any sign-in", () => {
    const one = walletLabel(wallet("Imported wallet", "aaaa1111"), undefined);
    const two = walletLabel(wallet("Imported wallet", "bbbb2222"), undefined);
    expect(one).toBe("Imported wallet · #aaaa");
    expect(two).not.toBe(one);
  });

  test("shows a short public key once one is known, and the username with it", () => {
    expect(walletLabel(wallet("Imported wallet"), { publicKey: KEY })).toBe(
      "Imported wallet · 0xabab…abab",
    );
    expect(
      walletLabel(wallet("Imported wallet"), {
        publicKey: KEY,
        username: "alice.07",
      }),
    ).toBe("alice.07 · 0xabab…abab");
  });

  test("keeps a name the user typed, with the username and key after it", () => {
    expect(
      walletLabel(wallet("Work"), { publicKey: KEY, username: "alice.07" }),
    ).toBe("Work · alice.07 · 0xabab…abab");
    expect(
      walletLabel(wallet("alice.07"), { publicKey: KEY, username: "alice.07" }),
    ).toBe("alice.07 · 0xabab…abab");
  });

  test("shortens a key and leaves a short one whole", () => {
    expect(shortKey(KEY)).toBe("0xabab…abab");
    expect(shortKey("0x12")).toBe("0x12");
  });
});

describe("wallet public info", () => {
  test("keeps the key and name per wallet, and forgets them with the wallet", () => {
    const store = new WalletPublicInfoStore(new MemoryStorage());
    store.update("a", { publicKey: KEY, username: "alice.07" });
    store.update("b", { publicKey: `0x${"cd".repeat(32)}` });
    expect(store.get("a")).toEqual({ publicKey: KEY, username: "alice.07" });
    expect(store.get("b")?.username).toBeUndefined();
    store.forget("a");
    expect(store.get("a")).toBeUndefined();
    expect(store.get("b")).toBeDefined();
  });

  // The name belonged to the old key, so a changed key must not keep it.
  test("keeps a known name for the same key and drops it for a different one", () => {
    const store = new WalletPublicInfoStore(new MemoryStorage());
    store.update("a", { publicKey: KEY, username: "alice.07" });
    store.update("a", { publicKey: KEY });
    expect(store.get("a")?.username).toBe("alice.07");
    store.update("a", { publicKey: `0x${"ee".repeat(32)}` });
    expect(store.get("a")?.username).toBeUndefined();
  });

  test("reads damaged storage as nothing known", () => {
    const backing = new MemoryStorage();
    backing.setItem("truapi-web-signing-host:wallet-public:v1:a", "{oops");
    expect(new WalletPublicInfoStore(backing).get("a")).toBeUndefined();
  });
});

describe("username from a reading", () => {
  const record = { fullUsername: "alice", liteUsername: "alice.07" };
  test("prefers the identity account and the full name, and says nothing when absent", () => {
    expect(
      usernameFrom([
        {
          role: "root",
          consumer: { fullUsername: null, liteUsername: "root.01" },
        },
        { role: "identity", consumer: record },
      ]),
    ).toBe("alice");
    expect(
      usernameFrom([
        { role: "identity", consumer: null },
        {
          role: "root",
          consumer: { fullUsername: null, liteUsername: "root.01" },
        },
      ]),
    ).toBe("root.01");
    expect(
      usernameFrom([{ role: "identity", consumer: null }]),
    ).toBeUndefined();
  });
});
