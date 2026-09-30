import { describe, expect, test } from "bun:test";
import {
  formatBytes,
  formatUnits,
  parseProductResourceStatus,
  resourceRows,
  type ProductResourceStatus,
  type ResourcesState,
} from "./product-resources.js";

const ACCOUNT = `0x${"aa".repeat(32)}`;
const BLOCK = `0x${"bb".repeat(32)}`;

const statementStore = (
  entries: { period: number; seq: number; state: string }[],
  graceSeconds: number | null = 3600,
) => ({
  status: "read",
  account: ACCOUNT,
  blockHash: BLOCK,
  currentPeriod: 20_000,
  graceSeconds,
  entries,
});

const bulletin = (authorization: object | null) => ({
  status: "read",
  account: ACCOUNT,
  blockHash: BLOCK,
  blockNumber: 500,
  authorization,
});

const pgas = (overrides: object = {}) => ({
  status: "read",
  account: ACCOUNT,
  accountIndex: 0,
  blockHash: BLOCK,
  assetId: 7,
  balance: "12345678900",
  claimAmount: "50000000000",
  decimals: 10,
  symbol: "PGAS",
  ...overrides,
});

const document = (parts: Partial<Record<string, unknown>> = {}) =>
  JSON.stringify({
    productId: "demo.paseo",
    statementStore: statementStore([]),
    bulletin: bulletin(null),
    pgas: pgas(),
    ...parts,
  });

const done = (json: string): ResourcesState => ({
  state: "done",
  productId: "demo.paseo",
  checkedAt: new Date(0),
  status: parseProductResourceStatus(json),
});

const rows = (json: string) =>
  Object.fromEntries(resourceRows(done(json)).map((row) => [row.label, row]));

describe("parsing the core's document", () => {
  test("keeps a part the chain answered apart from one it could not read", () => {
    const status: ProductResourceStatus = parseProductResourceStatus(
      document({
        bulletin: {
          status: "unavailable",
          account: ACCOUNT,
          reason: "no route",
        },
      }),
    );
    expect(status.bulletin).toEqual({
      status: "unavailable",
      account: ACCOUNT,
      reason: "no route",
    });
    expect(status.statementStore.status).toBe("read");
  });

  // A layout the page does not know must surface, not become "no allocation".
  test("refuses a document that is not the layout", () => {
    expect(() => parseProductResourceStatus("[]")).toThrow("layout");
    expect(() =>
      parseProductResourceStatus(document({ pgas: pgas({ balance: -1 }) })),
    ).toThrow("balance");
    expect(() =>
      parseProductResourceStatus(
        document({
          statementStore: statementStore([{ period: 1, seq: 0, state: "odd" }]),
        }),
      ),
    ).toThrow("state");
  });
});

describe("amounts", () => {
  test("places the decimal point without floating point", () => {
    expect(formatUnits("12345678900", 10)).toBe("1.23456789");
    expect(formatUnits("5", 10)).toBe("0.0000000005");
    expect(formatUnits("30000000000", 10)).toBe("3");
    expect(formatUnits("42", 0)).toBe("42");
    expect(formatUnits("340282366920938463463374607431768211455", 10)).toBe(
      "34028236692093846346337460743.1768211455",
    );
  });

  test("shows bytes in binary units", () => {
    expect(formatBytes("512")).toBe("512 B");
    expect(formatBytes("2097152")).toBe("2.0 MiB");
    expect(formatBytes("1536")).toBe("1.5 KiB");
    expect(formatBytes("1073741824")).toBe("1.0 GiB");
  });
});

describe("Statement Store row", () => {
  test("says no allocation when the index holds nothing, and no more", () => {
    expect(rows(document())["Statement Store"]).toMatchObject({
      value: "No allocation",
    });
  });

  test("names the slot of an entry for the current period", () => {
    expect(
      rows(
        document({
          statementStore: statementStore([
            { period: 20_000, seq: 2, state: "current" },
          ]),
        }),
      )["Statement Store"],
    ).toMatchObject({ value: "Allocated · this period", detail: "slot 2" });
  });

  test("tells an earlier period in its grace window from a lapsed one", () => {
    const row = (state: string) =>
      rows(
        document({
          statementStore: statementStore([{ period: 19_999, seq: 0, state }]),
        }),
      )["Statement Store"];
    expect(row("grace")).toMatchObject({ value: "In grace window" });
    expect(row("lapsed")).toMatchObject({
      value: "No current allocation",
      state: "warning",
    });
  });
});

describe("Bulletin row", () => {
  test("shows what remains and when it ends", () => {
    expect(
      rows(
        document({
          bulletin: bulletin({
            remainingTransactions: 7,
            remainingBytes: "2097152",
            expiresAtBlock: 900,
            expired: false,
          }),
        }),
      ).Bulletin,
    ).toMatchObject({ value: "7 tx · 2.0 MiB", detail: "until block 900" });
  });

  test("tells no record from an expired one", () => {
    expect(rows(document()).Bulletin).toMatchObject({
      value: "No authorization",
    });
    expect(
      rows(
        document({
          bulletin: bulletin({
            remainingTransactions: 1,
            remainingBytes: "10",
            expiresAtBlock: 400,
            expired: true,
          }),
        }),
      ).Bulletin,
    ).toMatchObject({ value: "Expired", state: "warning" });
  });
});

describe("PGAS row", () => {
  // A balance is a balance: the wording must not turn it into a permission or
  // a count of claims left.
  test("shows the balance in whole units of the asset", () => {
    const row = rows(document()).PGAS;
    expect(row).toMatchObject({
      value: "1.23456789 PGAS",
      detail: "account 0",
    });
    expect(row.title).toContain("not a permission");
  });

  test("shows raw units when the runtime holds no asset metadata", () => {
    expect(
      rows(document({ pgas: pgas({ decimals: null, symbol: null }) })).PGAS,
    ).toMatchObject({
      value: "12345678900 units",
      detail: "decimals not on chain",
    });
  });

  test("reports an unreadable chain as unavailable, not as zero", () => {
    expect(
      rows(
        document({
          pgas: {
            status: "unavailable",
            account: ACCOUNT,
            reason: "timed out",
          },
        }),
      ).PGAS,
    ).toMatchObject({ value: "Unavailable", state: "warning" });
  });
});

describe("before and around a read", () => {
  test("states each non-read state once", () => {
    const one = (state: ResourcesState) => resourceRows(state);
    expect(one({ state: "idle" })).toHaveLength(1);
    expect(one({ state: "loading", productId: "a" })[0].value).toBe(
      "Checking…",
    );
    expect(one({ state: "unsupported", productId: "a" })[0].value).toBe(
      "Needs a newer core",
    );
    expect(
      one({ state: "error", productId: "a", message: "no session" })[0],
    ).toMatchObject({ value: "Unavailable", detail: "no session" });
  });
});
