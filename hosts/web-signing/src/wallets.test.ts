import { describe, expect, test } from "bun:test";
import { WalletStore } from "./wallets.js";
import { MemoryStorage } from "./test-support.js";

const PHRASE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";

describe("WalletStore", () => {
  test("offers the built-in dev accounts before any wallet is saved", () => {
    const store = new WalletStore(new MemoryStorage());
    expect(store.list().map((wallet) => wallet.id)).toEqual([
      "dev:alice",
      "dev:bob",
      "dev:charlie",
      "dev:dave",
    ]);
  });

  // The wallet id is its storage namespace, so importing a phrase again must
  // land on the same state rather than an empty one.
  test("keeps one wallet per recovery phrase", () => {
    const store = new WalletStore(new MemoryStorage());
    const first = store.save("First", PHRASE);
    const again = store.save("Again", `  ${PHRASE.replaceAll(" ", "   ")}\n`);
    expect(again.id).toBe(first.id);
    expect(
      store.list().filter((wallet) => !wallet.id.startsWith("dev:")),
    ).toHaveLength(1);
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
    const store = new WalletStore(new MemoryStorage());
    const wallet = store.save("Gone", PHRASE);
    store.forget(wallet.id);
    expect(store.find(wallet.id)).toBeUndefined();
    expect(store.mnemonicOf(wallet.id)).toBeUndefined();
  });
});
