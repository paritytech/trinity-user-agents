import { describe, expect, test } from "bun:test";
import type { ChainProvider } from "@parity/truapi-host";
import {
  decodeAssetHubAccount,
  formatUnits,
  productChainRows,
  readBalance,
  type BalanceReading,
  type ProductChainState,
} from "./product-chain.js";
import { productAllowanceRows } from "./account-status.js";

const u128 = (value: bigint): number[] =>
  Array.from({ length: 16 }, (_, index) =>
    Number((value >> BigInt(8 * index)) & 0xffn),
  );

/** `AccountInfo`: four u32 counters, then free, reserved, frozen and flags. */
const accountInfo = (free: bigint, reserved = 0n, frozen = 0n): Uint8Array =>
  Uint8Array.from([
    ...new Uint8Array(16),
    ...u128(free),
    ...u128(reserved),
    ...u128(frozen),
    ...u128(0n),
  ]);

const ACCOUNT = `0x${"cd".repeat(32)}` as const;
const BLOCK = `0x${"ab".repeat(32)}` as const;
const done = (balance: BalanceReading): ProductChainState => ({
  state: "done",
  productId: "demo.paseo",
  account: ACCOUNT,
  balance,
});
const row = (state: ProductChainState, label: string) =>
  productChainRows(state).find((candidate) => candidate.label === label);

describe("balance record", () => {
  // Past 2^53 a Number would round, so a balance travels as a bigint.
  test("reads the free, reserved and frozen amounts exactly", () => {
    const free = 2n ** 100n + 7n;
    expect(decodeAssetHubAccount(accountInfo(free, 5n, 9n))).toEqual({
      free,
      reserved: 5n,
      frozen: 9n,
    });
  });

  test("refuses a record that is not this layout", () => {
    expect(() => decodeAssetHubAccount(new Uint8Array(40))).toThrow();
    expect(() =>
      decodeAssetHubAccount(Uint8Array.from([...accountInfo(1n), 0])),
    ).toThrow("unread");
  });

  test("places the decimal point without floating point", () => {
    expect(formatUnits(12345678900n, 10)).toBe("1.23456789");
    expect(formatUnits(5n, 10)).toBe("0.0000000005");
    expect(formatUnits(30000000000n, 10)).toBe("3");
    expect(formatUnits(42n, 0)).toBe("42");
    expect(formatUnits(2n ** 100n, 10)).toBe(
      "126765060022822940149.6703205376",
    );
  });
});

describe("rows", () => {
  test("states no record, zero and a balance as three different things", () => {
    const token = { decimals: 10, symbol: "PAS" };
    const read = (
      account: { free: bigint; reserved: bigint; frozen: bigint } | null,
    ) =>
      row(
        done({ tag: "Read", blockHash: BLOCK, account, token }),
        "Asset Hub balance",
      );
    expect(read(null)).toMatchObject({ value: "No account entry" });
    expect(read({ free: 0n, reserved: 0n, frozen: 0n })).toMatchObject({
      value: "0 PAS",
    });
    expect(
      read({ free: 15_000_000_000n, reserved: 0n, frozen: 0n }),
    ).toMatchObject({ value: "1.5 PAS", detail: "free" });
  });

  // With no token properties the amount must not be given a scale.
  test("shows the smallest unit when the chain gave no token properties", () => {
    expect(
      row(
        done({
          tag: "Read",
          blockHash: BLOCK,
          account: { free: 15n, reserved: 0n, frozen: 0n },
          token: null,
        }),
        "Asset Hub balance",
      ),
    ).toMatchObject({ value: "15 units", detail: "decimals not reported" });
  });

  test("keeps the account when only the balance could not be read", () => {
    const state = done({ tag: "Unavailable", reason: "no answer" });
    expect(row(state, "Account 0")?.value).toBe("0xcdcdcd…dcdcd");
    expect(row(state, "Account 0")?.title).toContain("derives nothing");
    expect(row(state, "Asset Hub balance")).toMatchObject({
      value: "Unavailable",
      state: "warning",
    });
  });

  test("states loading and failure without an account", () => {
    expect(productChainRows({ state: "idle" })[0].value).toBe("Not read");
    expect(
      productChainRows({
        state: "error",
        productId: "a",
        message: "no session",
      })[0],
    ).toMatchObject({ value: "Unavailable", detail: "no session" });
  });

  // A read made for another product must never be shown under this one.
  test("shows a read only under the product it was made for", () => {
    const view = {
      state: "done" as const,
      productId: "other.paseo",
      record: "none" as const,
    };
    expect(
      productAllowanceRows(view, done({ tag: "Unavailable", reason: "x" })).map(
        (candidate) => candidate.label,
      ),
    ).toEqual(["Product", "Allocation recorded", "Account 0"]);
    expect(
      productAllowanceRows(view, done({ tag: "Unavailable", reason: "x" }))[2],
    ).toMatchObject({ value: "Not read" });
  });
});

describe("reading the balance", () => {
  // A chain that cannot be reached is unknown, never zero.
  test("reports an unreachable chain as unavailable", async () => {
    const chain: ChainProvider = {
      connect: () => Promise.reject(new Error("no route")),
    };
    expect(await readBalance(chain, BLOCK, ACCOUNT)).toEqual({
      tag: "Unavailable",
      reason: "no route",
    });
  });
});
