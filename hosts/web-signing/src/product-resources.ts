import type { VersionRow } from "./versions.js";

/**
 * What the core reports about the public resource state of the open product.
 * The core reads it from the chains at their finalized blocks and returns it as
 * JSON, so the page only parses and words it. Each part stands alone: one chain
 * failing leaves the others.
 */

/** A part the chain answered, or the reason it did not. */
export type Reading<T> =
  | ({ status: "read" } & T)
  | { status: "unavailable"; account: string; reason: string };

export type AllowanceEntryState =
  | "current"
  | "grace"
  | "lapsed"
  | "earlier"
  | "ahead";

export interface StatementStoreReading {
  account: string;
  blockHash: string;
  currentPeriod: number;
  graceSeconds: number | null;
  entries: { period: number; seq: number; state: AllowanceEntryState }[];
}

export interface BulletinReading {
  account: string;
  blockHash: string;
  blockNumber: number;
  authorization: {
    remainingTransactions: number;
    remainingBytes: string;
    expiresAtBlock: number;
    expired: boolean;
  } | null;
}

export interface PgasReading {
  account: string;
  accountIndex: number;
  blockHash: string;
  assetId: number;
  balance: string;
  claimAmount: string;
  decimals: number | null;
  symbol: string | null;
}

export interface ProductResourceStatus {
  productId: string;
  statementStore: Reading<StatementStoreReading>;
  bulletin: Reading<BulletinReading>;
  pgas: Reading<PgasReading>;
}

/** Where the read of the open product's resources stands. */
export type ResourcesState =
  | { state: "idle" }
  | { state: "loading"; productId: string }
  | { state: "unsupported"; productId: string }
  | { state: "error"; productId: string; message: string }
  | {
      state: "done";
      productId: string;
      checkedAt: Date;
      status: ProductResourceStatus;
    };

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null;

function fail(what: string): never {
  throw new Error(
    `the core's resource status is not the layout this reads: ${what}`,
  );
}

function field<T>(
  record: Record<string, unknown>,
  name: string,
  check: (value: unknown) => value is T,
): T {
  const value = record[name];
  if (!check(value)) fail(name);
  return value;
}

const isString = (value: unknown): value is string => typeof value === "string";
const isNumber = (value: unknown): value is number =>
  typeof value === "number" && Number.isInteger(value) && value >= 0;
const isDecimal = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9]+$/.test(value);
const isBoolean = (value: unknown): value is boolean =>
  typeof value === "boolean";
const nullable =
  <T>(check: (value: unknown) => value is T) =>
  (value: unknown): value is T | null =>
    value === null || check(value);
const isEntryState = (value: unknown): value is AllowanceEntryState =>
  value === "current" ||
  value === "grace" ||
  value === "lapsed" ||
  value === "earlier" ||
  value === "ahead";

function readPart<T>(
  value: unknown,
  name: string,
  parse: (record: Record<string, unknown>) => T,
): Reading<T> {
  if (!isRecord(value)) fail(name);
  if (value.status === "unavailable")
    return {
      status: "unavailable",
      account: field(value, "account", isString),
      reason: field(value, "reason", isString),
    };
  if (value.status !== "read") fail(`${name}.status`);
  return { status: "read", ...parse(value) };
}

/** Parse the core's JSON document. Throws when it is not the layout this reads. */
export function parseProductResourceStatus(
  json: string,
): ProductResourceStatus {
  const root: unknown = JSON.parse(json);
  if (!isRecord(root)) fail("the document");
  return {
    productId: field(root, "productId", isString),
    statementStore: readPart(root.statementStore, "statementStore", (part) => ({
      account: field(part, "account", isString),
      blockHash: field(part, "blockHash", isString),
      currentPeriod: field(part, "currentPeriod", isNumber),
      graceSeconds: field(part, "graceSeconds", nullable(isNumber)),
      entries: field(part, "entries", (value): value is unknown[] =>
        Array.isArray(value),
      ).map((entry) => {
        if (!isRecord(entry)) fail("statementStore.entries");
        return {
          period: field(entry, "period", isNumber),
          seq: field(entry, "seq", isNumber),
          state: field(entry, "state", isEntryState),
        };
      }),
    })),
    bulletin: readPart(root.bulletin, "bulletin", (part) => {
      const record = part.authorization;
      return {
        account: field(part, "account", isString),
        blockHash: field(part, "blockHash", isString),
        blockNumber: field(part, "blockNumber", isNumber),
        authorization:
          record === null
            ? null
            : isRecord(record)
              ? {
                  remainingTransactions: field(
                    record,
                    "remainingTransactions",
                    isNumber,
                  ),
                  remainingBytes: field(record, "remainingBytes", isDecimal),
                  expiresAtBlock: field(record, "expiresAtBlock", isNumber),
                  expired: field(record, "expired", isBoolean),
                }
              : fail("bulletin.authorization"),
      };
    }),
    pgas: readPart(root.pgas, "pgas", (part) => ({
      account: field(part, "account", isString),
      accountIndex: field(part, "accountIndex", isNumber),
      blockHash: field(part, "blockHash", isString),
      assetId: field(part, "assetId", isNumber),
      balance: field(part, "balance", isDecimal),
      claimAmount: field(part, "claimAmount", isDecimal),
      decimals: field(part, "decimals", nullable(isNumber)),
      symbol: field(part, "symbol", nullable(isString)),
    })),
  };
}

/** `amount` (whole smallest units, as decimal text) shown with `decimals` places. */
export function formatUnits(amount: string, decimals: number): string {
  if (decimals === 0) return amount;
  const padded = amount.padStart(decimals + 1, "0");
  const whole = padded.slice(0, -decimals);
  const fraction = padded.slice(-decimals).replace(/0+$/, "");
  return fraction === "" ? whole : `${whole}.${fraction}`;
}

const BYTE_UNITS = ["B", "KiB", "MiB", "GiB", "TiB"];

/** A byte count, decimal text, in the largest binary unit that keeps it at one or more. */
export function formatBytes(bytes: string): string {
  const value = BigInt(bytes);
  if (value < 1024n) return `${value} B`;
  let unit = 1;
  let tenths = (value * 10n) / 1024n;
  while (tenths >= 10240n && unit < BYTE_UNITS.length - 1) {
    tenths /= 1024n;
    unit += 1;
  }
  return `${tenths / 10n}.${tenths % 10n} ${BYTE_UNITS[unit]}`;
}

const unavailableRow = (
  label: string,
  part: { account: string; reason: string },
): VersionRow => ({
  label,
  value: "Unavailable",
  detail: part.reason,
  state: "warning",
  title: `${part.reason}\nAccount ${part.account}\nNothing was changed. This is not a finding about what the chain holds.`,
});

function statementStoreRow(part: Reading<StatementStoreReading>): VersionRow {
  const label = "Statement Store";
  if (part.status === "unavailable") return unavailableRow(label, part);
  const note = `Resources.StmtStoreAllowanceByAccount of the product's allowance account ${part.account} on the People chain, at finalized block ${part.blockHash}. An entry means the chain holds an allowance naming that account. It is not remaining quota, and it does not say a statement will be accepted.`;
  const period = (state: AllowanceEntryState) =>
    part.entries.filter((entry) => entry.state === state);
  const current = period("current");
  if (current.length > 0)
    return {
      label,
      value: "Allocated · this period",
      detail: `slot ${current.map((entry) => entry.seq).join(", ")}`,
      title: `Period ${part.currentPeriod}.\n${note}`,
    };
  const grace = period("grace");
  if (grace.length > 0)
    return {
      label,
      value: "In grace window",
      detail: `period ${Math.max(...grace.map((entry) => entry.period))}, now ${part.currentPeriod}`,
      title: `Only an earlier period is held, still inside the runtime's grace window of ${part.graceSeconds} s. Renewal moves it to the current period.\n${note}`,
    };
  if (part.entries.length > 0) {
    const last = Math.max(...part.entries.map((entry) => entry.period));
    return {
      label,
      value: "No current allocation",
      detail: `last period ${last}, now ${part.currentPeriod}`,
      state: "warning",
      title: `Entries exist only for other periods (${part.entries.map((entry) => `${entry.period}/${entry.seq}`).join(", ")}).${part.graceSeconds === null ? " The grace window could not be read." : ""}\n${note}`,
    };
  }
  return {
    label,
    value: "No allocation",
    title: `No entry for this account at that block.\n${note}`,
  };
}

function bulletinRow(part: Reading<BulletinReading>): VersionRow {
  const label = "Bulletin";
  if (part.status === "unavailable") return unavailableRow(label, part);
  const note = `TransactionStorage.Authorizations of the product's allowance account ${part.account} on Bulletin, at finalized block ${part.blockHash} (number ${part.blockNumber}). Remaining is the authorization's allowance minus what it has used.`;
  const { authorization } = part;
  if (authorization === null)
    return {
      label,
      value: "No authorization",
      title: `No record for this account at that block.\n${note}`,
    };
  if (authorization.expired)
    return {
      label,
      value: "Expired",
      detail: `block ${authorization.expiresAtBlock}, now ${part.blockNumber}`,
      state: "warning",
      title: `${authorization.remainingTransactions} transactions and ${authorization.remainingBytes} bytes were left when it expired.\n${note}`,
    };
  return {
    label,
    value: `${authorization.remainingTransactions} tx · ${formatBytes(authorization.remainingBytes)}`,
    detail: `until block ${authorization.expiresAtBlock}`,
    title: `${authorization.remainingBytes} bytes left. Now block ${part.blockNumber}.\n${note}`,
  };
}

function pgasRow(part: Reading<PgasReading>): VersionRow {
  const label = "PGAS";
  if (part.status === "unavailable") return unavailableRow(label, part);
  const symbol = part.symbol ?? "PGAS";
  const shown =
    part.decimals === null
      ? { value: `${part.balance} units`, detail: "decimals not on chain" }
      : {
          value: `${formatUnits(part.balance, part.decimals)} ${symbol}`,
          detail: `account ${part.accountIndex}`,
        };
  const claim =
    part.decimals === null
      ? `${part.claimAmount} units`
      : `${formatUnits(part.claimAmount, part.decimals)} ${symbol}`;
  return {
    label,
    ...shown,
    title: `Balance of asset ${part.assetId} held by the product's account ${part.accountIndex} (${part.account}) on Asset Hub, at finalized block ${part.blockHash}. PGAS pays contract fees; this is a balance, not a permission or a remaining claim allowance. One claim mints ${claim}. Only account ${part.accountIndex} is read; a product may use others.`,
  };
}

/** Rows for the open product's resources, from what the last Refresh read. */
export function resourceRows(resources: ResourcesState): VersionRow[] {
  switch (resources.state) {
    case "idle":
      return [
        {
          label: "On chain",
          value: "Not read",
          detail: "press Refresh",
          state: "unknown",
          title:
            "Statement Store, Bulletin and PGAS are read from the chains when you press Refresh.",
        },
      ];
    case "loading":
      return [{ label: "On chain", value: "Checking…", state: "unknown" }];
    case "unsupported":
      return [
        {
          label: "On chain",
          value: "Needs a newer core",
          state: "unknown",
          title:
            "The loaded core build has no productResourceStatus export, so the product's Statement Store, Bulletin and PGAS state cannot be read. Rebuild the testing WASM bundle and reload.",
        },
      ];
    case "error":
      return [
        {
          label: "On chain",
          value: "Unavailable",
          detail: resources.message,
          state: "warning",
          title: `${resources.message}\nNothing was changed.`,
        },
      ];
    case "done":
      return [
        statementStoreRow(resources.status.statementStore),
        bulletinRow(resources.status.bulletin),
        pgasRow(resources.status.pgas),
      ];
  }
}
