import { describe, expect, test } from "bun:test";
import {
  allocationRecord,
  decodeRenewalLedger,
  normalizeProductId,
} from "./allowance-ledger.js";

const text = (value: string) => [
  value.length << 2,
  ...new TextEncoder().encode(value),
];
const OWNER = [1, ...new Uint8Array(32).fill(5)];

/** `Vec<TrackedStatementRenewalTarget>` as the core stores it. */
const ledger = (...entries: number[][]) =>
  Uint8Array.from([entries.length << 2, ...entries.flat()]);
const product = (id: string) => [0, ...text(id), 0];
const walletSso = [1, 0];
const account = [2, ...new Uint8Array(32).fill(3), ...text("peer"), ...OWNER];

describe("renewal ledger", () => {
  test("reads product recipes, the wallet account and raw accounts", () => {
    expect(
      decodeRenewalLedger(ledger(product("app.paseo"), walletSso, account)),
    ).toEqual([
      { tag: "ProductStatementAllowance", productId: "app.paseo" },
      { tag: "WalletSso" },
      { tag: "Account", label: "peer" },
    ]);
  });

  test("refuses bytes that are not that layout", () => {
    expect(() => decodeRenewalLedger(Uint8Array.of(4, 9, 0))).toThrow(
      "unknown ledger target",
    );
    expect(() => decodeRenewalLedger(Uint8Array.of(4))).toThrow();
    expect(() =>
      decodeRenewalLedger(Uint8Array.from([...ledger(walletSso), 0])),
    ).toThrow("unread");
  });
});

describe("allocation record", () => {
  // The core derives and records the normalized id, so a differently cased or
  // padded id must still find its entry.
  test("finds a product by the id the core normalizes it to", () => {
    const bytes = ledger(product("my-app.paseo"));
    expect(allocationRecord(bytes, "  My-App.PASEO ")).toBe("recorded");
    expect(normalizeProductId(" Cafe\u0301.paseo")).toBe("caf\u00e9.paseo");
  });

  // Another product's entry, or the wallet's own, says nothing about this one.
  test("does not take another target for this product", () => {
    const bytes = ledger(product("other.paseo"), walletSso, account);
    expect(allocationRecord(bytes, "app.paseo")).toBe("none");
  });

  test("reads an empty or missing ledger as none, and a broken one as unreadable", () => {
    expect(allocationRecord(undefined, "app.paseo")).toBe("none");
    expect(allocationRecord(ledger(), "app.paseo")).toBe("none");
    expect(allocationRecord(Uint8Array.of(0xff, 0xff), "app.paseo")).toBe(
      "unreadable",
    );
  });
});
