import { hexToBytes, type HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import { allowanceKey } from "./chain-directory.js";
import { readStorageValues } from "./people-chain.js";
import { Reader } from "./scale-reader.js";

/**
 * What the People runtime grants a Statement Store account, read at the
 * newest finalized block, and the wait that holds an account's first post
 * until its claim is final there.
 *
 * A grant read opens a connection, follows the chain and reads storage only:
 * it opens no statement subscription. In this host's light client
 * (smoldot-light 2.1.0) every People connection shares one network service,
 * and each connection's statement subscriptions replace the one topic filter
 * that service sends to its peers (`json_rpc_service/background.rs:922-929`,
 * `network_service.rs:2314-2334`). A connection schedules a filter update on
 * a statement subscribe or unsubscribe alone (`background.rs:3146`, `3178`;
 * a new one has none pending, `:734-736`), and closing it drops the shared
 * filter only when it was the chain's last connection (`lib.rs:1010-1020`,
 * `network_service.rs:1769`), which the core's own connection is not.
 */

/** The People chain, as a grant read reaches it. */
export interface PeopleAccess {
  chain: ChainProvider;
  genesis: HexString;
}

/** A `StatementAllowance` as the runtime stores it, or null when none is. */
export type Grant = { maxCount: number; maxSize: number } | null;

/** The SCALE `StatementAllowance { max_count: u32, max_size: u32 }`. */
export function decodeGrant(hex: string): {
  maxCount: number;
  maxSize: number;
} {
  const reader = new Reader(hexToBytes(hex));
  const grant = { maxCount: reader.u32(), maxSize: reader.u32() };
  reader.finish();
  return grant;
}

/** `:statement_allowance:` of each of `accounts` at the newest finalized People block. */
export async function readGrants(
  access: PeopleAccess,
  accounts: string[],
): Promise<{ block: string; values: { account: string; value: Grant }[] }> {
  const connection = await access.chain.connect(hexToBytes(access.genesis));
  const { blockHash, values } = await readStorageValues(
    connection,
    accounts.map((account) => allowanceKey(hexToBytes(account))),
    undefined,
    [],
    "People",
  );
  return {
    block: blockHash,
    values: accounts.map((account, index) => {
      const value = values[index];
      return { account, value: value === null ? null : decodeGrant(value) };
    }),
  };
}

/** What {@link untilFinalizedAllowance} saw when the allowance held at the finalized head. */
export interface SettledAllowance {
  account: string;
  /** The block the claim landed in, as the core reported it. */
  claimBlock: string | undefined;
  seq: number;
  collection: string;
  /** The finalized block the allowance was read at. */
  finalizedBlock: string;
  grant: NonNullable<Grant>;
  /** From the claim's answer to the read that found it. */
  waitedMs: number;
  reads: number;
}

export interface FinalizedAllowanceOptions {
  /** Read the account's `:statement_allowance:` at the newest finalized block. */
  read(account: string): Promise<{ block: string; grant: Grant }>;
  /** Wait between reads. */
  sleep(ms: number): Promise<void>;
  now(): number;
  /** Whether the session the claim was made for still runs. */
  running(): boolean;
  timeoutMs?: number;
  intervalMs?: number;
  /**
   * How the timeout names the account and what the user may do. Absent: a
   * chat's conversation account, which the user posts again by sending.
   */
  wording?: { account: string; retry: string };
}

/** How long a claim may take to finalize before the wait gives up. */
export const FINALITY_TIMEOUT_MS = 180_000;
const FINALITY_INTERVAL_MS = 3_000;

/**
 * Wait until `claim`'s account holds an allowance at the People chain's
 * finalized head. A node accepts a statement against the allowance at its
 * best block, but its periodic limit check reads the finalized block and
 * evicts an account's statements that have no allowance there, so a
 * statement posted between inclusion and finality may be accepted and then
 * dropped. Rejects, saying why, after {@link FINALITY_TIMEOUT_MS} or when
 * the session ends.
 */
export async function untilFinalizedAllowance(
  claim: StatementAccountClaim,
  options: FinalizedAllowanceOptions,
): Promise<SettledAllowance> {
  const start = options.now();
  const timeoutMs = options.timeoutMs ?? FINALITY_TIMEOUT_MS;
  let reads = 0;
  let last = "no read finished";
  for (;;) {
    if (!options.running())
      throw new Error(
        "The session ended before the chat's allowance was final.",
      );
    try {
      const { block, grant } = await options.read(claim.account);
      reads += 1;
      if (grant !== null && grant.maxCount > 0 && grant.maxSize > 0)
        return {
          account: claim.account,
          claimBlock: claim.blockHash,
          seq: claim.seq,
          collection: claim.collection,
          finalizedBlock: block,
          grant,
          waitedMs: options.now() - start,
          reads,
        };
      last = `finalized block ${block} has no allowance for it yet`;
    } catch (error) {
      last = `the People chain could not be read: ${error instanceof Error ? error.message : String(error)}`;
    }
    if (options.now() - start >= timeoutMs)
      throw new Error(
        `${options.wording?.account ?? "This chat's account"} ${claim.account} has its slot (${claim.collection} ${claim.seq}${claim.blockHash === undefined ? "" : `, block ${claim.blockHash}`}), but it was not final after ${Math.round(timeoutMs / 1000)} s: ${last}. Nothing was posted; ${options.wording?.retry ?? "send again to retry"}.`,
      );
    await options.sleep(options.intervalMs ?? FINALITY_INTERVAL_MS);
  }
}
