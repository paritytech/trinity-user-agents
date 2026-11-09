import { hexToBytes, type HexString } from "@parity/truapi/scale";
import type { ChainProvider, JsonRpcConnection } from "@parity/truapi-host";
import { allowanceKey } from "./chain-directory.js";
import { abortion, readStorageValues } from "./people-chain.js";
import { Reader } from "./scale-reader.js";

/**
 * What the People runtime grants a Statement Store account, read at the
 * newest finalized block, and the wait until a claim's allowance holds
 * there. `./allowance-usable.ts` decides when an account may post: at once
 * after this wait, or earlier, at a best block, with this wait running
 * behind it.
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

/** Open an allowance read within 10 s; close any connection arriving after cancellation or timeout. */
export async function connectAllowanceRead(
  access: PeopleAccess,
  signal?: AbortSignal,
): Promise<JsonRpcConnection> {
  if (signal?.aborted) throw new Error("the read was stopped");
  const { aborted, release } = abortion(signal);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error("the allowance connection timed out")),
      10_000,
    );
  });
  let opening: Promise<JsonRpcConnection> | undefined;
  let handedOut = false;
  try {
    opening = access.chain.connect(hexToBytes(access.genesis));
    const connection = await Promise.race([opening, timeout, aborted]);
    if (signal?.aborted) throw new Error("the read was stopped");
    handedOut = true;
    return connection;
  } finally {
    clearTimeout(timer);
    release();
    if (!handedOut && opening !== undefined)
      void opening.then(
        (connection) => connection.close(),
        () => {},
      );
  }
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

/**
 * `:statement_allowance:` of each of `accounts` at the newest finalized
 * People block. Rejects once `signal` aborts, and closes its connection.
 */
export async function readGrants(
  access: PeopleAccess,
  accounts: string[],
  signal?: AbortSignal,
): Promise<{ block: string; values: { account: string; value: Grant }[] }> {
  if (signal?.aborted) throw new Error("the read was stopped");
  const connection = await connectAllowanceRead(access, signal);
  if (signal?.aborted) {
    connection.close();
    throw new Error("the read was stopped");
  }
  const { blockHash, values } = await readStorageValues(
    connection,
    accounts.map((account) => allowanceKey(hexToBytes(account))),
    undefined,
    [],
    "People",
    signal,
  );
  return {
    block: blockHash,
    values: accounts.map((account, index) => {
      const value = values[index];
      return { account, value: value === null ? null : decodeGrant(value) };
    }),
  };
}

/** Read a grant and prove a known claim is on finalized canonical ancestry, on one RPC connection. */
export async function readClaimGrant(
  access: PeopleAccess,
  account: string,
  claimBlock: string,
  signal?: AbortSignal,
): Promise<{
  block: string;
  grant: Grant;
  claimFinal: boolean;
  claimLost: boolean;
}> {
  if (signal?.aborted) throw new Error("the read was stopped");
  const connection = await connectAllowanceRead(access, signal);
  if (signal?.aborted) {
    connection.close();
    throw new Error("the read was stopped");
  }
  const { aborted, release } = abortion(signal);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error("the claim ancestry read timed out")),
      10_000,
    );
  });
  const call = (id: number, method: string, params: unknown[]) =>
    connection.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
  const run = async () => {
    call(1, "chain_getFinalizedHead", []);
    call(2, "chain_getHeader", [claimBlock]);
    let block: string | undefined;
    let claimNumber: bigint | undefined;
    let finalNumber: bigint | undefined;
    let canonical: string | undefined;
    let grant: Grant = null;
    let read = false;
    for await (const text of connection.responses()) {
      const message = JSON.parse(text) as {
        id?: number;
        result?: unknown;
        error?: { message?: string };
      };
      if (message.error)
        throw new Error(
          message.error.message ?? "the ancestry read was refused",
        );
      if (message.id === 1) {
        if (typeof message.result !== "string")
          throw new Error("no finalized block");
        block = message.result;
        call(3, "chain_getHeader", [block]);
        call(5, "state_getStorage", [allowanceKey(hexToBytes(account)), block]);
      } else if (message.id === 2 || message.id === 3) {
        const header = message.result as { number?: unknown } | null;
        if (
          typeof header?.number !== "string" ||
          !/^0x[0-9a-f]+$/i.test(header.number)
        )
          throw new Error(
            "the node could not place the claim on finalized ancestry",
          );
        if (message.id === 2) {
          claimNumber = BigInt(header.number);
          call(4, "chain_getBlockHash", [header.number]);
        } else finalNumber = BigInt(header.number);
      } else if (message.id === 4) {
        if (typeof message.result !== "string")
          throw new Error("no canonical claim height");
        canonical = message.result;
      } else if (message.id === 5) {
        if (message.result !== null && typeof message.result !== "string")
          throw new Error("no allowance answer");
        grant = message.result === null ? null : decodeGrant(message.result);
        read = true;
      }
      if (
        block !== undefined &&
        claimNumber !== undefined &&
        finalNumber !== undefined &&
        canonical !== undefined &&
        read
      ) {
        const reached = finalNumber >= claimNumber;
        const matches = canonical.toLowerCase() === claimBlock.toLowerCase();
        return {
          block,
          grant,
          claimFinal: reached && matches,
          claimLost: reached && !matches,
        };
      }
    }
    throw new Error("the ancestry connection closed");
  };
  try {
    return await Promise.race([run(), timeout, aborted]);
  } finally {
    clearTimeout(timer);
    release();
    connection.close();
  }
}

/**
 * What a guard needs of a claim: the account, its slot and day, and the
 * block the claim landed in when the core registered it just now. A core
 * `StatementAccountClaim` is one; a notification claim names no collection
 * when the account was already registered.
 */
export interface ClaimedAccount {
  /** The account, `0x` hex. */
  account: string;
  period: number;
  seq: number;
  collection?: string;
  /** The block the claim landed in, once registered now. */
  blockHash?: string;
}

/** What {@link untilFinalizedAllowance} saw when the allowance held at the finalized head. */
export interface SettledAllowance {
  account: string;
  /** The block the claim landed in, as the core reported it. */
  claimBlock: string | undefined;
  /** True only when the known inclusion block is on finalized ancestry. */
  claimFinal: boolean;
  seq: number;
  collection: string | undefined;
  /** The finalized block the allowance was read at. */
  finalizedBlock: string;
  grant: NonNullable<Grant>;
  /** From the claim's answer to the read that found it. */
  waitedMs: number;
  reads: number;
}

export interface FinalizedAllowanceOptions {
  /** Read the account's `:statement_allowance:` at the newest finalized block. */
  read(account: string): Promise<{
    block: string;
    grant: Grant;
    claimFinal?: boolean;
    claimLost?: boolean;
  }>;
  /** Require inclusion ancestry as well as a grant, not a grant left from yesterday. */
  requireClaimFinality?: boolean;
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

/** Whether `grant` lets its account post at all. */
export const isUsableGrant = (grant: Grant): grant is NonNullable<Grant> =>
  grant !== null && grant.maxCount > 0 && grant.maxSize > 0;

/**
 * Wait until `claim`'s account holds an allowance at the People chain's
 * finalized head, reading every {@link FinalizedAllowanceOptions.intervalMs}.
 * Answers what it saw, or why it stopped: after
 * {@link FINALITY_TIMEOUT_MS}, with the last read's outcome, or when the
 * session ended. Never rejects.
 */
export async function finalizedOrWhyNot(
  claim: ClaimedAccount,
  options: FinalizedAllowanceOptions,
): Promise<
  | { settled: SettledAllowance }
  | { why: string; ended: boolean; timeoutMs: number }
> {
  const start = options.now();
  const timeoutMs = options.timeoutMs ?? FINALITY_TIMEOUT_MS;
  let reads = 0;
  let last = "no read finished";
  for (;;) {
    if (!options.running())
      return { why: "the session ended", ended: true, timeoutMs };
    try {
      const {
        block,
        grant,
        claimFinal = false,
        claimLost = false,
      } = await options.read(claim.account);
      reads += 1;
      if (!options.running())
        return { why: "the session ended", ended: true, timeoutMs };
      if (options.requireClaimFinality && claimLost)
        return {
          why: `claim block ${claim.blockHash} is not on finalized ancestry`,
          ended: false,
          timeoutMs,
        };
      if (isUsableGrant(grant) && (!options.requireClaimFinality || claimFinal))
        return {
          settled: {
            account: claim.account,
            claimBlock: claim.blockHash,
            claimFinal,
            seq: claim.seq,
            collection: claim.collection,
            finalizedBlock: block,
            grant,
            waitedMs: options.now() - start,
            reads,
          },
        };
      last = isUsableGrant(grant)
        ? `finalized block ${block} has an allowance, but does not confirm claim block ${claim.blockHash}`
        : `finalized block ${block} has no allowance for it yet`;
    } catch (error) {
      last = `the People chain could not be read: ${error instanceof Error ? error.message : String(error)}`;
    }
    if (options.now() - start >= timeoutMs)
      return { why: last, ended: false, timeoutMs };
    await options.sleep(options.intervalMs ?? FINALITY_INTERVAL_MS);
  }
}

/**
 * Wait until `claim`'s account holds an allowance at the People chain's
 * finalized head. Before its periodic limit check took the best block into
 * account too (polkadot-sdk #13437), a node evicted an account's statements
 * that had no allowance at the finalized block, so a statement posted
 * between inclusion and finality could be accepted and then dropped; nodes
 * without that change still do. Rejects, saying why, after
 * {@link FINALITY_TIMEOUT_MS} or when the session ends.
 */
export async function untilFinalizedAllowance(
  claim: ClaimedAccount,
  options: FinalizedAllowanceOptions,
): Promise<SettledAllowance> {
  const start = options.now();
  const outcome = await finalizedOrWhyNot(claim, options);
  if ("settled" in outcome) return outcome.settled;
  if (outcome.ended)
    throw new Error("The session ended before the chat's allowance was final.");
  const slot = `${claim.collection === undefined ? "" : `${claim.collection} `}${claim.seq}${claim.blockHash === undefined ? "" : `, block ${claim.blockHash}`}`;
  throw new Error(
    `${options.wording?.account ?? "This chat's account"} ${claim.account} has its slot (${slot}), but it was not final after ${Math.round((options.now() - start) / 1000)} s: ${outcome.why}. Nothing was posted; ${options.wording?.retry ?? "send again to retry"}.`,
  );
}
