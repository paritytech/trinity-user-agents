import { describe, expect, it } from "bun:test";
import {
  validateWalletAllowanceSnapshot,
  type WalletAllowanceSnapshot,
} from "./wallet-allowances.js";

const account = `0x${"11".repeat(32)}`;
const observation = {
  genesisHash: `0x${"22".repeat(32)}`,
  blockHash: `0x${"33".repeat(32)}`,
  blockNumber: 42,
  specVersion: 1,
  chainTimestamp: 1_800_000_000,
};
function snapshot(): WalletAllowanceSnapshot {
  return {
    schemaVersion: 1,
    identityAccountId: account,
    networkSuffix: "paseo",
    productIds: ["chat.dot"],
    statementStore: {
      status: "available",
      observation,
      value: {
        period: 4,
        resetsAt: 1_800_100_000,
        graceSeconds: 30,
        replacementCooldownSeconds: 60,
        pools: [
          {
            collection: "LitePeople",
            membership: "verified",
            selected: true,
            limit: 2,
            used: 1,
            remaining: 1,
            slots: [{ index: 0, accountId: account }],
          },
        ],
      },
    },
    pgasClaims: { status: "unavailable", reason: "chain unavailable" },
    pgasBalances: {
      status: "available",
      observation,
      value: {
        assetId: "1984",
        decimals: null,
        symbol: null,
        accounts: [
          {
            productId: "chat.dot",
            accountId: account,
            derivationIndex: 0,
            balance: "9007199254740993123456",
          },
        ],
      },
    },
    bulletinClaims: { status: "unavailable", reason: "chain unavailable" },
    bulletinQuotas: { status: "unavailable", reason: "chain unavailable" },
  };
}

describe("native wallet allowance snapshot validation", () => {
  it("rejects malformed capacity instead of exposing a fabricated free slot", () => {
    const value = snapshot();
    if (value.statementStore.status !== "available") throw new Error("fixture");
    value.statementStore.value.pools[0]!.remaining = 2;
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
  });

  it("never treats a nonmember policy limit as usable capacity", () => {
    const value = snapshot();
    if (value.statementStore.status !== "available") throw new Error("fixture");
    const pool = value.statementStore.value.pools[0]!;
    pool.membership = "not-found";
    pool.selected = false;
    pool.remaining = 0;
    validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]);
    pool.remaining = pool.limit - pool.used;
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
    pool.remaining = 0;
    pool.selected = true;
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
  });

  it("rejects imprecise numbers and missing chain provenance", () => {
    const value = snapshot();
    if (value.statementStore.status !== "available") throw new Error("fixture");
    value.statementStore.observation = {
      ...observation,
      blockNumber: Number.MAX_SAFE_INTEGER + 1,
    };
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
    value.statementStore.observation = { ...observation, blockHash: "" };
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
  });

  it("requires separate People provenance for Asset Hub claim eligibility", () => {
    const value = snapshot();
    value.pgasClaims = {
      status: "available",
      observation,
      value: {
        period: 4,
        resetsAt: 1_800_100_000,
        pools: [],
        assetId: "1984",
        claimAmount: "100",
        membershipObservation: { ...observation, genesisHash: account },
      },
    };
    validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]);
    value.pgasClaims.value.membershipObservation.blockHash = "";
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
  });

  it("rejects partial account batches instead of treating omissions as zero", () => {
    const value = snapshot();
    if (value.pgasBalances.status !== "available") throw new Error("fixture");
    value.pgasBalances.value.accounts = [];
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
  });

  it("rejects lossy numeric balances and unexplained null balances", () => {
    const value = snapshot();
    validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]);
    if (value.pgasBalances.status !== "available") throw new Error("fixture");
    const numericBalance = {
      ...value,
      pgasBalances: {
        ...value.pgasBalances,
        value: {
          ...value.pgasBalances.value,
          accounts: [
            {
              ...value.pgasBalances.value.accounts[0],
              balance: 9007199254740992,
            },
          ],
        },
      },
    };
    expect(() =>
      validateWalletAllowanceSnapshot(numericBalance, account, "paseo", [
        "chat.dot",
      ]),
    ).toThrow();
    value.pgasBalances.value.accounts[0]!.balance = null;
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]),
    ).toThrow();
    value.pgasBalances.value.accounts[0]!.error = "RPC unavailable";
    validateWalletAllowanceSnapshot(value, account, "paseo", ["chat.dot"]);
  });

  it("rejects snapshots from another wallet, network, schema or product scope", () => {
    const value = snapshot();
    expect(() =>
      validateWalletAllowanceSnapshot(value, `0x${"44".repeat(32)}`, "paseo", [
        "chat.dot",
      ]),
    ).toThrow();
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "dot", ["chat.dot"]),
    ).toThrow();
    expect(() =>
      validateWalletAllowanceSnapshot(value, account, "paseo", ["other.dot"]),
    ).toThrow();
    expect(() =>
      validateWalletAllowanceSnapshot(
        { ...value, schemaVersion: 2 },
        account,
        "paseo",
        ["chat.dot"],
      ),
    ).toThrow();
  });
});
