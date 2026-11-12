import { describe, expect, test } from "bun:test";
import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { toHex, utf8 } from "../../../bridges/primitives/bytes.js";
import { LocketError } from "../../../bridges/locket/protocol.js";
import { MAX_ACCOUNTS, RecipientKeys } from "../../../src/locket/keys.js";
import { x25519Pair } from "../../../src/invitations/sealing.js";

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
const PRODUCT = "market.example";
const INDEX_A = `0x${"0a".repeat(32)}`;
const INDEX_B = `0x${"0b".repeat(32)}`;
const ACCOUNT_A = `0x${"aa".repeat(32)}`;
const ACCOUNT_B = `0x${"bb".repeat(32)}`;

const code = (run: () => unknown): string | null => {
  try {
    run();
    return null;
  } catch (error) {
    return error instanceof LocketError ? error.code : String(error);
  }
};

/** Store a key for `account` at `index`, as the host does once the core resolved it. */
const make = (keys: RecipientKeys, index: string, account: string) =>
  keys.keep(PRODUCT, index, account, keys.prepare());

describe("recipient keys", () => {
  test("one key per account, kept across a restart, opening only with the same wallet", () => {
    const storage = memoryStorage();
    const first = new RecipientKeys(storage, random);
    first.unlock(owner());
    const a = make(first, INDEX_A, ACCOUNT_A);
    const b = make(first, INDEX_B, ACCOUNT_B);
    expect({
      distinctKey: a.publicKey !== b.publicKey,
      distinctId: a.keyId !== b.keyId,
      foundA: first.find(PRODUCT, INDEX_A),
      again: code(() => make(first, INDEX_A, ACCOUNT_A)),
    }).toEqual({
      distinctKey: true,
      distinctId: true,
      foundA: { ...a, account: ACCOUNT_A },
      again: "busy",
    });

    const restarted = new RecipientKeys(storage, random);
    expect(code(() => restarted.open(PRODUCT, a.keyId))).toBe("session");
    restarted.unlock(owner());
    const opened = restarted.open(PRODUCT, b.keyId)!;
    expect({
      publicKey: toHex(opened.pair.public),
      accounts: opened.accounts,
      legacy: opened.legacy,
      otherProduct: restarted.open("other.example", b.keyId),
    }).toEqual({
      publicKey: b.publicKey,
      accounts: [ACCOUNT_B],
      legacy: false,
      otherProduct: null,
    });

    // The same wallet id with other entropy, as after a phrase change, opens nothing.
    const other = new RecipientKeys(storage, random);
    other.unlock(owner(2));
    expect(code(() => other.open(PRODUCT, a.keyId))).toBe("unavailable");
  });

  test("stores no secret in clear, and a record moved to another account or product does not open", () => {
    const storage = memoryStorage();
    const keys = new RecipientKeys(storage, random);
    keys.unlock(owner());
    const a = make(keys, INDEX_A, ACCOUNT_A);
    const secret = toHex(keys.open(PRODUCT, a.keyId)!.pair.secret);
    expect(
      [...storage.items.values()].join("\n").includes(secret.slice(2)),
    ).toBe(false);

    const [slot, record] = [...storage.items][0]!;
    storage.setItem(
      slot,
      record.replace(ACCOUNT_A.slice(2), ACCOUNT_B.slice(2)),
    );
    expect(code(() => keys.open(PRODUCT, a.keyId))).toBe("unavailable");
    storage.setItem(slot.replace(PRODUCT, "other.example"), record);
    expect(code(() => keys.open("other.example", a.keyId))).toBe("unavailable");
  });

  test("lock keeps the records; unlocking again opens them; accounts are bounded", () => {
    const storage = memoryStorage();
    const keys = new RecipientKeys(storage, random);
    keys.unlock(owner());
    const a = make(keys, INDEX_A, ACCOUNT_A);
    keys.lock();
    expect({
      locked: code(() => keys.prepare()),
      kept: storage.items.size,
    }).toEqual({ locked: "session", kept: 1 });
    keys.unlock(owner());
    expect(keys.open(PRODUCT, a.keyId)?.publicKey).toBe(a.publicKey);
    for (let index = 1; index < MAX_ACCOUNTS; index++)
      make(
        keys,
        `0x${index.toString(16).padStart(64, "0")}`,
        `0x${index.toString(16).padStart(64, "f")}`,
      );
    expect(code(() => make(keys, INDEX_B, ACCOUNT_B))).toBe("limit");
  });

  test("an unreadable record is kept: no new key is written over it, and nothing reports it missing", () => {
    const storage = memoryStorage();
    const keys = new RecipientKeys(storage, random);
    keys.unlock(owner());
    const a = make(keys, INDEX_A, ACCOUNT_A);
    const [slot, record] = [...storage.items][0]!;
    // One entry damaged on disk beside a good one, as a cut write leaves it.
    const damaged = JSON.stringify([
      ...JSON.parse(record),
      { keyId: "0x12", account: ACCOUNT_B },
    ]);
    storage.setItem(slot, damaged);
    const results = {
      writeOther: code(() => make(keys, INDEX_B, ACCOUNT_B)),
      findOther: code(() => keys.find(PRODUCT, INDEX_B)),
      openUnknown: code(() => keys.open(PRODUCT, `0x${"77".repeat(16)}`)),
      openGood: keys.open(PRODUCT, a.keyId)?.publicKey === a.publicKey,
      findGood: keys.find(PRODUCT, INDEX_A)?.keyId === a.keyId,
      kept: storage.getItem(slot) === damaged,
    };
    // A legacy record that does not parse is kept and reported the same way.
    const legacySlot = slot.slice(0, -":accounts".length);
    storage.setItem(slot, record);
    storage.setItem(legacySlot, "{not json");
    expect({
      ...results,
      legacyUnknown: code(() => keys.open(PRODUCT, `0x${"77".repeat(16)}`)),
      legacyKept: storage.getItem(legacySlot),
    }).toEqual({
      writeOther: "unavailable",
      findOther: "unavailable",
      openUnknown: "unavailable",
      openGood: true,
      findGood: true,
      kept: true,
      legacyUnknown: "unavailable",
      legacyKept: "{not json",
    });
  });

  test("a legacy shared key still opens, read only, for the accounts it was bound to", () => {
    const storage = memoryStorage();
    // A record as earlier builds stored it: one key per product, in the product's slot.
    const wrap = hkdf(
      sha256,
      owner().entropy,
      utf8("test-net"),
      utf8("truapi-web-host/fields/v1/receive-key-wrap"),
      32,
    );
    const secret = random(32);
    const keyId = toHex(random(16));
    const publicKey = toHex(x25519Pair(secret).public);
    const nonce = random(24);
    storage.setItem(
      `truapi-web-signing-host:fields:v1:test-net:wallet-a:${PRODUCT}`,
      JSON.stringify({
        keyId,
        publicKey,
        created: 1,
        accounts: [ACCOUNT_A, ACCOUNT_B],
        nonce: toHex(nonce),
        sealed: toHex(
          xchacha20poly1305(
            wrap,
            nonce,
            utf8(
              `web-fields/receive-key/v1|test-net|${PRODUCT}|${keyId}|${publicKey}`,
            ),
          ).encrypt(secret),
        ),
      }),
    );
    const keys = new RecipientKeys(storage, random);
    keys.unlock(owner());
    const legacy = keys.open(PRODUCT, keyId)!;
    expect({
      opens: toHex(legacy.pair.public) === publicKey,
      legacy: legacy.legacy,
      accounts: legacy.accounts,
      // A new key for an account never reuses it.
      found: keys.find(PRODUCT, INDEX_A),
      fresh: make(keys, INDEX_A, ACCOUNT_A).keyId !== keyId,
    }).toEqual({
      opens: true,
      legacy: true,
      accounts: [ACCOUNT_A, ACCOUNT_B],
      found: null,
      fresh: true,
    });
  });
});
