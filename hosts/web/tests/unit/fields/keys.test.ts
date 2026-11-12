import { describe, expect, test } from "bun:test";
import { toHex } from "../../../bridges/primitives/bytes.js";
import { FieldError } from "../../../bridges/fields/protocol.js";
import { ReceiveKeys } from "../../../src/fields/keys.js";

/** A `Storage` subset over a plain map, as one browser profile's `localStorage`. */
function memoryStorage() {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
  };
}

let seed = 3;
const random = (length: number) =>
  Uint8Array.from({ length }, () => (seed = (seed * 37 + 11) % 251));
const owner = (entropy = 1) => ({
  network: "test-net",
  walletId: "wallet-a",
  entropy: new Uint8Array(16).fill(entropy),
});
const ACCOUNT = `0x${"ab".repeat(32)}`;

const code = (run: () => unknown): string | null => {
  try {
    run();
    return null;
  } catch (error) {
    return error instanceof FieldError ? error.code : String(error);
  }
};

describe("receiving keys", () => {
  test("one key per product, kept across a restart, opening only with the same wallet", () => {
    const storage = memoryStorage();
    const first = new ReceiveKeys(storage, random);
    first.unlock(owner());
    const made = first.ensure("market.example");
    first.bound("market.example", ACCOUNT);
    expect(first.ensure("market.example").keyId).toBe(made.keyId);

    const restarted = new ReceiveKeys(storage, random);
    expect(code(() => restarted.open("market.example", made.keyId))).toBe(
      "session",
    );
    restarted.unlock(owner());
    const opened = restarted.open("market.example", made.keyId)!;
    expect({
      publicKey: toHex(opened.pair.public),
      accounts: opened.accounts,
      otherProduct: restarted.open("other.example", made.keyId),
    }).toEqual({
      publicKey: made.publicKey,
      accounts: [ACCOUNT],
      otherProduct: null,
    });

    // The same wallet id with other entropy, as after a phrase change, opens nothing.
    const other = new ReceiveKeys(storage, random);
    other.unlock(owner(2));
    expect(code(() => other.open("market.example", made.keyId))).toBe(
      "invalid",
    );
  });

  test("stores no secret in clear, and a record moved to another product does not open", () => {
    const storage = memoryStorage();
    const keys = new ReceiveKeys(storage, random);
    keys.unlock(owner());
    const made = keys.ensure("market.example");
    const secret = toHex(keys.open("market.example", made.keyId)!.pair.secret);
    const stored = [...storage.items.values()].join("\n");
    expect(stored.includes(secret.slice(2))).toBe(false);

    const [slot, record] = [...storage.items][0]!;
    storage.setItem(slot.replace("market.example", "other.example"), record);
    expect(code(() => keys.open("other.example", made.keyId))).toBe("invalid");
  });

  test("lock keeps the record; unlocking again opens it", () => {
    const storage = memoryStorage();
    const keys = new ReceiveKeys(storage, random);
    keys.unlock(owner());
    const made = keys.ensure("market.example");
    keys.lock();
    expect({
      locked: code(() => keys.ensure("market.example")),
      kept: storage.items.size,
    }).toEqual({ locked: "session", kept: 1 });
    keys.unlock(owner());
    expect(keys.open("market.example", made.keyId)?.publicKey).toBe(
      made.publicKey,
    );
  });
});
