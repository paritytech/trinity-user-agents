export type AllowanceCollection = "People" | "LitePeople";
export interface AllowanceObservation {
  genesisHash: string;
  blockHash: string;
  blockNumber: number;
  specVersion: number;
  /** Unix seconds from the pinned block. */
  chainTimestamp: number;
}
export type AllowanceSection<T> =
  | { status: "available"; observation: AllowanceObservation; value: T }
  | { status: "unavailable"; reason: string };
export interface AllowanceSlot {
  index: number;
  accountId?: string;
  productId?: string;
  label?: string;
  since?: number;
}
export interface AllowancePool {
  collection: AllowanceCollection;
  membership: "verified" | "not-found";
  selected: boolean;
  limit: number;
  used: number;
  remaining: number;
  slots: AllowanceSlot[];
}
export interface AllowanceClaims {
  period: number;
  resetsAt: number;
  pools: AllowancePool[];
}
export interface StatementAllowanceSnapshot extends AllowanceClaims {
  graceSeconds: number;
  replacementCooldownSeconds: number;
}
export interface PgasClaimsSnapshot extends AllowanceClaims {
  /** Membership is observed on People, separately from Asset Hub claim storage. */
  membershipObservation: AllowanceObservation;
  assetId: string;
  /** Integer base units, never a floating-point amount. */
  claimAmount: string;
}
export interface PgasBalancesSnapshot {
  assetId: string;
  decimals: number | null;
  symbol: string | null;
  accounts: Array<{
    productId: string;
    accountId: string;
    derivationIndex: 0;
    /** Total balance, not a spendability promise. */
    balance: string | null;
    error?: string;
  }>;
}
export interface BulletinQuota {
  productId: string;
  accountId: string;
  status: "active" | "expired" | "missing" | "unavailable";
  bytesUsed?: string;
  bytesLimit?: string;
  bytesRemaining?: string;
  transactionsUsed?: number;
  transactionsLimit?: number;
  transactionsRemaining?: number;
  expiresAtBlock?: number;
  error?: string;
}
export interface WalletAllowanceSnapshot {
  schemaVersion: 1;
  identityAccountId: string;
  networkSuffix: string;
  productIds: string[];
  statementStore: AllowanceSection<StatementAllowanceSnapshot>;
  pgasClaims: AllowanceSection<PgasClaimsSnapshot>;
  pgasBalances: AllowanceSection<PgasBalancesSnapshot>;
  bulletinClaims: AllowanceSection<AllowanceClaims>;
  bulletinQuotas: AllowanceSection<{ accounts: BulletinQuota[] }>;
}

/** Bound trusted shell hints before crossing the worker boundary. Native code
 * applies the canonical product-ID rules before deriving or querying accounts. */
export function validateAllowanceProductIds(value: unknown): string[] {
  if (
    !Array.isArray(value) ||
    value.length > 32 ||
    new Set(value).size !== value.length
  ) {
    throw new Error(
      "allowance inspection requires at most 32 unique nonempty product IDs",
    );
  }
  for (const id of value) {
    if (typeof id !== "string" || id.trim().length === 0) {
      throw new Error("allowance inspection requires nonempty product IDs");
    }
  }
  return [...value];
}

function requireValid(condition: unknown): asserts condition {
  if (!condition) throw new Error("invalid wallet allowance snapshot");
}
function record(value: unknown): Record<string, unknown> {
  requireValid(
    typeof value === "object" && value !== null && !Array.isArray(value),
  );
  return value as Record<string, unknown>;
}
function text(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}
function uint(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}
function units(value: unknown): value is string {
  return typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value);
}
function hash(value: unknown): value is string {
  return typeof value === "string" && /^0x[0-9a-f]{64}$/.test(value);
}
function array(value: unknown): unknown[] {
  requireValid(Array.isArray(value));
  return value;
}
function claims(value: Record<string, unknown>): void {
  requireValid(uint(value.period) && uint(value.resetsAt));
  const collections = new Set<unknown>();
  for (const item of array(value.pools)) {
    const pool = record(item);
    requireValid(
      (pool.collection === "People" || pool.collection === "LitePeople") &&
        !collections.has(pool.collection),
    );
    collections.add(pool.collection);
    requireValid(
      (pool.membership === "verified" || pool.membership === "not-found") &&
        typeof pool.selected === "boolean" &&
        uint(pool.limit) &&
        uint(pool.used) &&
        uint(pool.remaining) &&
        (pool.membership === "not-found"
          ? pool.remaining === 0 && pool.selected === false
          : pool.remaining === Math.max(0, pool.limit - pool.used)),
    );
    const indices = new Set<unknown>();
    for (const entry of array(pool.slots)) {
      const slot = record(entry);
      requireValid(
        uint(slot.index) &&
          !indices.has(slot.index) &&
          (slot.accountId === undefined || hash(slot.accountId)) &&
          (slot.productId === undefined || text(slot.productId)) &&
          (slot.label === undefined || text(slot.label)) &&
          (slot.since === undefined || uint(slot.since)),
      );
      indices.add(slot.index);
    }
  }
}
function observation(value: unknown): void {
  const item = record(value);
  requireValid(
    hash(item.genesisHash) &&
      hash(item.blockHash) &&
      uint(item.blockNumber) &&
      uint(item.specVersion) &&
      uint(item.chainTimestamp),
  );
}
function section(
  value: unknown,
  validate: (value: Record<string, unknown>) => void,
): void {
  const item = record(value);
  if (item.status === "unavailable") {
    requireValid(text(item.reason));
    return;
  }
  requireValid(item.status === "available");
  observation(item.observation);
  validate(record(item.value));
}

/** Validate once at the native JSON boundary, before treating an observation as
 * current wallet data. Malformed data is rejected, never converted to capacity. */
export function validateWalletAllowanceSnapshot(
  value: unknown,
  identityAccountId: string,
  networkSuffix: string,
  productIds: readonly string[],
): asserts value is WalletAllowanceSnapshot {
  const snapshot = record(value);
  requireValid(
    snapshot.schemaVersion === 1 &&
      hash(snapshot.identityAccountId) &&
      snapshot.identityAccountId === identityAccountId &&
      snapshot.networkSuffix === networkSuffix,
  );
  const requested = new Set(productIds);
  const actual = validateAllowanceProductIds(snapshot.productIds);
  requireValid(
    actual.length === requested.size && actual.every((id) => requested.has(id)),
  );
  const accounts = (
    value: unknown,
    validate: (account: Record<string, unknown>) => void,
  ) => {
    const seen = new Set<unknown>();
    for (const entry of array(value)) {
      const account = record(entry);
      requireValid(
        text(account.productId) &&
          requested.has(account.productId) &&
          !seen.has(account.productId) &&
          hash(account.accountId),
      );
      seen.add(account.productId);
      validate(account);
    }
    requireValid(seen.size === requested.size);
  };
  section(snapshot.statementStore, (value) => {
    claims(value);
    requireValid(
      uint(value.graceSeconds) && uint(value.replacementCooldownSeconds),
    );
  });
  section(snapshot.pgasClaims, (value) => {
    claims(value);
    observation(value.membershipObservation);
    requireValid(text(value.assetId) && units(value.claimAmount));
  });
  section(snapshot.pgasBalances, (value) => {
    requireValid(
      text(value.assetId) &&
        (value.decimals === null || uint(value.decimals)) &&
        (value.symbol === null || text(value.symbol)),
    );
    accounts(value.accounts, (account) => {
      requireValid(
        account.derivationIndex === 0 &&
          (units(account.balance) ||
            (account.balance === null && text(account.error))) &&
          (account.error === undefined || text(account.error)),
      );
    });
  });
  section(snapshot.bulletinClaims, claims);
  section(snapshot.bulletinQuotas, (value) =>
    accounts(value.accounts, (account) => {
      requireValid(
        ["active", "expired", "missing", "unavailable"].includes(
          String(account.status),
        ),
      );
      for (const key of ["bytesUsed", "bytesLimit", "bytesRemaining"]) {
        requireValid(account[key] === undefined || units(account[key]));
      }
      for (const key of [
        "transactionsUsed",
        "transactionsLimit",
        "transactionsRemaining",
        "expiresAtBlock",
      ]) {
        requireValid(account[key] === undefined || uint(account[key]));
      }
      requireValid(account.error === undefined || text(account.error));
      if (account.status === "unavailable") requireValid(text(account.error));
      if (account.status === "active" || account.status === "expired") {
        requireValid(
          units(account.bytesUsed) &&
            units(account.bytesLimit) &&
            units(account.bytesRemaining) &&
            uint(account.transactionsUsed) &&
            uint(account.transactionsLimit) &&
            uint(account.transactionsRemaining) &&
            uint(account.expiresAtBlock),
        );
      }
    }),
  );
}
