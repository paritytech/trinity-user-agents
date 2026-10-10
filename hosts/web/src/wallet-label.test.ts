import { describe, expect, test } from "bun:test";
import {
  WalletPublicInfoStore,
  sessionUsernameFrom,
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

  // Sign-in activates the session with this name, so it must survive the
  // label updates that follow and belong only to the key it was read for.
  test("keeps the session username apart from the label name", () => {
    const store = new WalletPublicInfoStore(new MemoryStorage());
    store.update("a", { publicKey: KEY, sessionUsername: "alice.07" });
    store.update("a", { publicKey: KEY, username: "alice" });
    expect(store.get("a")).toEqual({
      publicKey: KEY,
      username: "alice",
      sessionUsername: "alice.07",
    });
    store.update("a", { publicKey: KEY, sessionUsername: null });
    expect(store.get("a")).toEqual({ publicKey: KEY, username: "alice" });
    store.update("a", { publicKey: KEY, sessionUsername: "alice.07" });
    store.update("a", { publicKey: `0x${"ee".repeat(32)}` });
    expect(store.get("a")?.sessionUsername).toBeUndefined();
  });

  test("reads damaged storage as nothing known", () => {
    const backing = new MemoryStorage();
    backing.setItem("truapi-web-signing-host:wallet-public:v1:a", "{oops");
    expect(new WalletPublicInfoStore(backing).get("a")).toBeUndefined();
  });
});

describe("wallet public info per network", () => {
  test("keeps the default network's original key", () => {
    const backing = new MemoryStorage();
    new WalletPublicInfoStore(backing).update("a", { publicKey: KEY });
    expect(backing.getItem("truapi-web-signing-host:wallet-public:v1:a")).toBe(
      JSON.stringify({ publicKey: KEY }),
    );
  });

  // A username learned on one network must not show on another.
  test("keeps a name per network for the same wallet", () => {
    const store = new WalletPublicInfoStore(new MemoryStorage());
    store.update("a", { publicKey: KEY, username: "alice.07" }, "paseo");
    expect(store.get("a", "paseo")?.username).toBe("alice.07");
    expect(store.get("a", "previewnet")).toBeUndefined();
    store.update("a", { publicKey: KEY }, "previewnet");
    expect(store.get("a", "previewnet")).toEqual({ publicKey: KEY });
    expect(store.get("a", "paseo")?.username).toBe("alice.07");
  });

  test("forgets a wallet on every network", () => {
    const store = new WalletPublicInfoStore(new MemoryStorage());
    store.update("a", { publicKey: KEY, username: "alice.07" }, "paseo");
    store.update("a", { publicKey: KEY }, "previewnet");
    store.update("b", { publicKey: KEY }, "previewnet");
    store.forget("a");
    expect(store.get("a", "paseo")).toBeUndefined();
    expect(store.get("a", "previewnet")).toBeUndefined();
    expect(store.get("b", "previewnet")).toBeDefined();
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

describe("session username from a reading", () => {
  const lite = (liteUsername: string) => ({ liteUsername });

  // The core answers account.getUserId with the full name when a session has
  // one, but a local session only takes a lite name, so the lite name of the
  // identity account is the one to activate with.
  test("takes the identity account's lite name before the root key's", () => {
    expect(
      sessionUsernameFrom([
        { role: "root", consumer: lite("root.01") },
        { role: "identity", consumer: lite("alice.07") },
      ]),
    ).toBe("alice.07");
    expect(
      sessionUsernameFrom([
        { role: "identity", consumer: null },
        { role: "root", consumer: lite("root.01") },
      ]),
    ).toBe("root.01");
  });

  test("says no name when neither account has a record", () => {
    expect(
      sessionUsernameFrom([
        { role: "identity", consumer: null },
        { role: "root", consumer: null },
      ]),
    ).toBeNull();
  });

  // An unreadable record is not an absent one: removing a name the session
  // carries because a decode failed would be wrong.
  test("says nothing when the deciding record could not be decoded", () => {
    expect(
      sessionUsernameFrom([
        { role: "identity", consumer: null, problem: "not the layout" },
        { role: "root", consumer: lite("root.01") },
      ]),
    ).toBeUndefined();
    expect(
      sessionUsernameFrom([
        { role: "identity", consumer: lite("alice.07") },
        { role: "root", consumer: null, problem: "not the layout" },
      ]),
    ).toBe("alice.07");
  });
});
