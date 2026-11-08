import { describe, expect, test } from "bun:test";
import { productSlotPrefix } from "../../../src/platform/storage.js";
import {
  applySnapshot,
  collectSnapshot,
  isFresh,
  readSnapshot,
} from "../../../src/backup/snapshot.js";
import { pointerExpiry } from "../../../src/backup/pointer.js";
import { MemoryStorage } from "../../support/test-support.js";

const A = { network: "paseo", walletId: "wallet-a" };
const B = { network: "paseo", walletId: "wallet-b" };
const record = (scope: typeof A, kind: string, rest = ":v1") =>
  `truapi-web-signing-host:network:${scope.network}:${kind}:${scope.walletId}${rest}`;
const product = (scope: typeof A, key: string) =>
  `${productSlotPrefix(scope.walletId, scope.network)}truapi:product-storage:v1:${key}`;

/** A browser with one wallet's state, and what a backup must leave alone beside it. */
function browserWithState(): MemoryStorage {
  const storage = new MemoryStorage();
  storage.setItem(record(A, "notifications"), '[{"id":"n1"}]');
  storage.setItem(
    record(A, "requests", ":contact-chat:sent:v1:day-inbox"),
    "[sent]",
  );
  storage.setItem(record(A, "nova-chat"), '{"outgoing":[]}');
  storage.setItem(record(A, "chat"), "[contacts]");
  storage.setItem(record(A, "product-chat"), "[rooms]");
  storage.setItem(
    product(A, "17:kettle-chat.paseo:kettle-chat:node:0011:state"),
    "0xc0ffee",
  );
  // Not the wallet's state on this network, or not a backup's to copy.
  storage.setItem(record(B, "notifications"), "[other wallet]");
  storage.setItem(
    "truapi-web-signing-host:network:previewnet:notifications:wallet-a:v1",
    "[other network]",
  );
  storage.setItem("truapi-web-signing-host:wallets:v1", "[phrases]");
  storage.setItem("truapi-web-signing-host:core:wallet-a:0x09", "0xdevice");
  storage.setItem("truapi-web-signing-host:network:paseo:dock:v2", "[dock]");
  storage.setItem(
    `${productSlotPrefix(A.walletId, A.network)}not-product-storage`,
    "x",
  );
  return storage;
}

describe("what a backup copies", () => {
  test("the wallet's host records and product storage on this network, without its wallet id, and nothing else", () => {
    const snapshot = collectSnapshot(browserWithState(), A);
    expect(snapshot).toEqual({
      records: [
        ["chat", ":v1", "[contacts]"],
        ["notifications", ":v1", '[{"id":"n1"}]'],
        ["nova-chat", ":v1", '{"outgoing":[]}'],
        ["product-chat", ":v1", "[rooms]"],
        ["requests", ":contact-chat:sent:v1:day-inbox", "[sent]"],
      ],
      products: [
        [
          "truapi:product-storage:v1:17:kettle-chat.paseo:kettle-chat:node:0011:state",
          "0xc0ffee",
        ],
      ],
    });
  });

  test("a restore writes the snapshot under the other browser's wallet id, and reads back the same snapshot", () => {
    const snapshot = collectSnapshot(browserWithState(), A);
    const other = new MemoryStorage();
    expect(isFresh(other, B)).toBe(true);
    applySnapshot(other, B, snapshot);
    expect(other.getItem(record(B, "nova-chat"))).toBe('{"outgoing":[]}');
    expect(collectSnapshot(other, B)).toEqual(snapshot);
    expect(other.keys().every((key) => !key.includes("wallet-a"))).toBe(true);
  });

  test("a restore never writes over a wallet this browser already holds", () => {
    const storage = browserWithState();
    const before = storage.keys().map((key) => [key, storage.getItem(key)]);
    expect(() =>
      applySnapshot(storage, A, collectSnapshot(storage, A)),
    ).toThrow("never replaces it");
    expect(storage.keys().map((key) => [key, storage.getItem(key)])).toEqual(
      before,
    );
  });

  test("a restore that cannot finish leaves nothing of itself", () => {
    const snapshot = collectSnapshot(browserWithState(), A);
    const full = new MemoryStorage();
    let room = 3;
    full.setItem = (key, value) => {
      if (room-- <= 0) throw new Error("QuotaExceededError");
      MemoryStorage.prototype.setItem.call(full, key, value);
    };
    expect(() => applySnapshot(full, B, snapshot)).toThrow(
      "nothing of it was kept",
    );
    expect(full.keys()).toEqual([]);
  });

  test("a snapshot naming a record this host does not keep, or a key outside product storage, is refused whole", () => {
    expect(() =>
      readSnapshot({
        records: [["wallets", ":v1", "[phrases]"]],
        products: [],
      }),
    ).toThrow("not one this host keeps");
    expect(() =>
      readSnapshot({ records: [], products: [["core:0x09", "0xdevice"]] }),
    ).toThrow("not one the core keeps");
    expect(() =>
      readSnapshot({
        records: [["chat", "-other-wallet:v1", "[x]"]],
        products: [],
      }),
    ).toThrow("not one this host keeps");
    const key = "truapi:product-storage:v1:17:kettle-chat.paseo:state";
    expect(
      readSnapshot({ records: [], products: [[key, "0xc0ffee"]] }).products,
    ).toEqual([[key, "0xc0ffee"]]);
    for (const [entry, why] of [
      [
        ["truapi:product-storage:v1:99:kettle-chat.paseo:state", "0x00"],
        "a product id of another length",
      ],
      [[key, "not hex"], "a value that is not hex"],
      [[key, "0xabc"], "an odd number of hex digits"],
    ] as const)
      expect(
        () => readSnapshot({ records: [], products: [entry] }),
        why,
      ).toThrow("not one the core keeps");
    expect(() =>
      readSnapshot({
        records: [],
        products: [
          [key, "0x01"],
          [key, "0x02"],
        ],
      }),
    ).toThrow("names one key twice");
  });
});

describe("a pointer's expiry", () => {
  const taken = Date.UTC(2026, 10, 8);

  test("lapses fourteen days after its snapshot was taken, with the revision as its low half", () => {
    const expiry = pointerExpiry(3, null, taken);
    expect(expiry >> 32n).toBe(BigInt(taken / 1000 + 14 * 86_400));
    expect(expiry & 0xffff_ffffn).toBe(3n);
  });

  test("is above the one it replaces, even one a later clock wrote", () => {
    const ahead = pointerExpiry(5, null, taken + 3_600_000);
    const next = pointerExpiry(6, ahead, taken);
    expect(next > ahead).toBe(true);
    expect(next & 0xffff_ffffn).toBe(6n);
    const same = pointerExpiry(5, ahead, taken + 3_600_000);
    expect(same > ahead).toBe(true);
  });
});
