import { hexToBytes, type HexString } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { allowanceKey } from "./chain-directory.js";
import {
  FINALITY_TIMEOUT_MS,
  connectAllowanceRead,
  decodeGrant,
  finalizedOrWhyNot,
  isUsableGrant,
  readClaimGrant,
  readGrants,
  untilFinalizedAllowance,
  type ClaimedAccount,
  type FinalizedAllowanceOptions,
  type Grant,
  type PeopleAccess,
} from "./allowance-finality.js";
import type { ChainBackend, NetworkConfig } from "./network-config.js";
import { abortion } from "./people-chain.js";

/**
 * When a Statement Store account may post after its claim: the network's
 * posting rule and the wait it sets.
 *
 * A node accepts a statement against the account's allowance at its own best
 * block. Whether it then keeps the statement while the claim is not final
 * depends on its version: since polkadot-sdk #13437 its periodic limit check
 * takes the larger of the best and the finalized allowance; before, it
 * evicted against the finalized one alone. So a network posts at the best
 * block only where its config permits that People chain and the actual RPC
 * connection reports the pinned genesis, SDK version and required methods.
 * Everywhere else an
 * account posts once its allowance holds at the finalized head.
 *
 * On a best-block network the wait is: the core registered the account just
 * now and verified the slot at the block the claim landed in; that block has
 * a child on the best chain; and the allowance holds at that best block. The
 * finalized wait then runs behind the post, bounded, and says whether the
 * claim became final. An account the core found registered already, or a
 * claim whose block cannot be followed, waits for the finalized head.
 */

/** How long a best-block wait looks for a child of the claim's block before it waits for the finalized head instead. */
export const CHILD_TIMEOUT_MS = 30_000;

/**
 * How long an answer given at a best block holds before it is asked again:
 * the finalized wait's own bound. Asked again, an account whose claim became
 * final answers at once at the finalized head.
 */
export const PROVISIONAL_MS = FINALITY_TIMEOUT_MS;

/** Whether one actual RPC connection reports the pinned chain, SDK and posting methods. */
export function checkedPostingNode(
  expectedGenesis: string,
  genesis: unknown,
  version: unknown,
  rpc: unknown,
): boolean {
  const methods = (rpc as { methods?: unknown } | undefined)?.methods;
  return (
    String(genesis).toLowerCase() === expectedGenesis.toLowerCase() &&
    /^1\.25\.1-4a0cc2e7449(?:$|[-+])/.test(String(version)) &&
    Array.isArray(methods) &&
    [
      "chainHead_v1_follow",
      "chainHead_v1_storage",
      "chain_getHeader",
      "chain_getBlockHash",
      "chain_getFinalizedHead",
      "state_getStorage",
      "statement_submit",
    ].every((method) => methods.includes(method))
  );
}

/** Where this network lets an account post after its claim, and why. */
export type PostingRule =
  | { wait: "best"; why: string }
  | { wait: "finalized"; why: string };

/**
 * The posting rule of `network`, reached through `peopleBackend`, whose
 * People chain has `peopleGenesis`.
 */
export function postingRule(
  network: NetworkConfig,
  peopleBackend: ChainBackend,
  peopleGenesis: string,
): PostingRule {
  const checked = network.bestBlockPosting;
  if (checked === null)
    return {
      wait: "finalized",
      why: `${network.displayName}'s People nodes are not checked for keeping statements of an allowance that is not final`,
    };
  if (peopleBackend !== "rpc")
    return {
      wait: "finalized",
      why: "People runs on the light client, whose peers are not the checked RPC nodes",
    };
  if (peopleGenesis.toLowerCase() !== checked.peopleGenesis.toLowerCase())
    return {
      wait: "finalized",
      why: `the People chain's genesis ${peopleGenesis} is not the one checked`,
    };
  return {
    wait: "best",
    why: `${network.displayName} permits a best-block read only after the connected RPC node's identity and capabilities pass`,
  };
}

/** What a best-block read found for a claim's block. */
export type BestRead =
  /** The claim's block has a child at the best block `block`, where the account holds `grant`. */
  | { tag: "child"; block: string; grant: Grant }
  /** The claim's block is final already. */
  | { tag: "final" }
  /** No best block descended from the claim's block within the wait. */
  | { tag: "unseen" };

/**
 * Read `key` at the first best block that descends, by at least one block,
 * from `claimBlock`, over `connection` with `chainHead_v1`. Answers
 * `final` when `claimBlock` is finalized first, and `unseen` after
 * `timeoutMs`. Closes the connection when done; rejects when the chain stops
 * following, refuses the read, or `signal` aborts.
 */
export async function readAfterChild(
  connection: JsonRpcConnection,
  key: HexString,
  claimBlock: string,
  timeoutMs: number = CHILD_TIMEOUT_MS,
  signal?: AbortSignal,
  expectedGenesis?: string,
): Promise<
  | { tag: "child"; block: string; value: HexString | null }
  | { tag: "final" }
  | { tag: "unseen" }
> {
  if (signal?.aborted) {
    connection.close();
    throw new Error("the read was stopped");
  }
  const claimed = claimBlock.toLowerCase();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timedOut = new Promise<{ tag: "unseen" }>((resolve) => {
    timer = setTimeout(() => resolve({ tag: "unseen" }), timeoutMs);
  });
  let subscription: string | undefined;
  const call = (id: number, method: string, params: unknown[]): void =>
    connection.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));

  const run = async (): Promise<
    | { tag: "child"; block: string; value: HexString | null }
    | { tag: "final" }
    | { tag: "unseen" }
  > => {
    const identity = new Map<number, unknown>();
    if (expectedGenesis === undefined) call(1, "chainHead_v1_follow", [false]);
    else {
      call(10, "chain_getBlockHash", [0]);
      call(11, "system_version", []);
      call(12, "rpc_methods", []);
    }
    const parents = new Map<string, string>();
    let best: string | undefined;
    let reading: string | undefined;
    let operation: string | undefined;
    let value: HexString | null = null;
    let claimNumber: bigint | undefined;
    let finalNumber: bigint | undefined;
    const descendsFrom = (block: string, ancestor: string): boolean => {
      for (let at = parents.get(block); at !== undefined; at = parents.get(at))
        if (at === ancestor) return true;
      return false;
    };
    const readIfDescends = () => {
      if (
        reading !== undefined ||
        best === undefined ||
        !descendsFrom(best, claimed)
      )
        return;
      reading = best;
      call(2, "chainHead_v1_storage", [
        subscription,
        reading,
        [{ key, type: "value" }],
        null,
      ]);
    };
    const lower = (hash: unknown) => String(hash).toLowerCase();
    for await (const text of connection.responses()) {
      const message = JSON.parse(text) as {
        id?: number;
        result?: unknown;
        error?: { message?: string };
        method?: string;
        params?: { subscription?: string; result?: Record<string, unknown> };
      };
      if (message.error !== undefined)
        throw new Error(message.error.message ?? "the chain refused a request");
      if (message.id !== undefined && message.id >= 10 && message.id <= 12) {
        identity.set(message.id, message.result);
        if (identity.size === 3) {
          if (
            expectedGenesis === undefined ||
            !checkedPostingNode(
              expectedGenesis,
              identity.get(10),
              identity.get(11),
              identity.get(12),
            )
          )
            throw new Error(
              "the connected People node's genesis, version or RPC capabilities are not checked for best-block posting",
            );
          call(1, "chainHead_v1_follow", [false]);
        }
        continue;
      }
      if (message.id === 13 || message.id === 14) {
        const header = message.result as { number?: unknown } | null;
        if (
          typeof header?.number !== "string" ||
          !/^0x[0-9a-f]+$/i.test(header.number)
        )
          throw new Error("the node could not place the claim block");
        if (message.id === 13) claimNumber = BigInt(header.number);
        else finalNumber = BigInt(header.number);
        if (
          claimNumber !== undefined &&
          finalNumber !== undefined &&
          finalNumber >= claimNumber
        )
          call(15, "chain_getBlockHash", [`0x${claimNumber.toString(16)}`]);
        continue;
      }
      if (message.id === 15)
        return String(message.result).toLowerCase() === claimed
          ? { tag: "final" }
          : { tag: "unseen" };
      if (message.id === 1) {
        if (typeof message.result !== "string")
          throw new Error("the chain gave no follow subscription");
        subscription = message.result;
        continue;
      }
      if (message.id === 2) {
        const started = message.result as
          | { result?: string; operationId?: string }
          | undefined;
        if (started?.result !== "started" || !started.operationId)
          throw new Error("the chain could not start the storage read");
        operation = started.operationId;
        continue;
      }
      if (
        message.method !== "chainHead_v1_followEvent" ||
        message.params?.subscription !== subscription
      )
        continue;
      const event = message.params?.result ?? {};
      switch (event.event) {
        case "initialized": {
          const hashes = (
            (event.finalizedBlockHashes as unknown[] | undefined) ?? [
              event.finalizedBlockHash,
            ]
          ).map(lower);
          if (hashes.includes(claimed)) return { tag: "final" };
          best = hashes[hashes.length - 1];
          if (expectedGenesis !== undefined && best !== undefined) {
            call(13, "chain_getHeader", [claimed]);
            call(14, "chain_getHeader", [best]);
          }
          break;
        }
        case "newBlock":
          parents.set(lower(event.blockHash), lower(event.parentBlockHash));
          readIfDescends();
          break;
        case "bestBlockChanged":
          best = lower(event.bestBlockHash);
          readIfDescends();
          break;
        case "finalized": {
          const pruned = (
            (event.prunedBlockHashes as unknown[] | undefined) ?? []
          ).map(lower);
          if (
            pruned.includes(claimed) ||
            (reading !== undefined && pruned.includes(reading))
          )
            return { tag: "unseen" };
          if (
            reading === undefined &&
            ((event.finalizedBlockHashes as unknown[] | undefined) ?? [])
              .map(lower)
              .includes(claimed)
          )
            return { tag: "final" };
          break;
        }
        case "operationStorageItems":
          if (event.operationId !== operation) break;
          for (const item of event.items as {
            key: string;
            value?: HexString;
          }[])
            if (item.key.toLowerCase() === key.toLowerCase())
              value = item.value ?? null;
          break;
        case "operationWaitingForContinue":
          if (event.operationId === operation)
            call(3, "chainHead_v1_continue", [subscription, operation]);
          break;
        case "operationStorageDone":
          if (event.operationId === operation && reading !== undefined) {
            if (
              best === reading ||
              (best !== undefined && descendsFrom(best, reading))
            )
              return { tag: "child", block: reading, value };
            reading = undefined;
            operation = undefined;
            value = null;
            readIfDescends();
          }
          break;
        case "operationInaccessible":
        case "operationError":
          if (event.operationId === operation)
            throw new Error("the chain could not serve the storage read");
          break;
        case "stop":
          throw new Error("the chain stopped following before it answered");
      }
    }
    throw new Error("the connection closed before the chain answered");
  };

  const { aborted, release } = abortion(signal);
  try {
    return await Promise.race([run(), timedOut, aborted]);
  } finally {
    clearTimeout(timer);
    release();
    if (subscription !== undefined)
      try {
        call(4, "chainHead_v1_unfollow", [subscription]);
      } catch {
        // the connection is already gone
      }
    connection.close();
  }
}

/** `account`'s allowance after a child of `claimBlock`, on the People chain behind `access`. */
export async function readGrantAfterChild(
  access: PeopleAccess,
  account: string,
  claimBlock: string,
  timeoutMs: number = CHILD_TIMEOUT_MS,
  signal?: AbortSignal,
): Promise<BestRead> {
  if (signal?.aborted) throw new Error("the read was stopped");
  const connection = await connectAllowanceRead(access, signal);
  const read = await readAfterChild(
    connection,
    allowanceKey(hexToBytes(account as HexString)),
    claimBlock,
    timeoutMs,
    signal,
    access.genesis,
  );
  return read.tag === "child"
    ? {
        tag: "child",
        block: read.block,
        grant: read.value === null ? null : decodeGrant(read.value),
      }
    : read;
}

/** When an account may post, as {@link untilUsableAllowance} found it. */
export interface UsableAllowance {
  account: string;
  seq: number;
  collection: string | undefined;
  /** The block the claim landed in, when the core registered it just now. */
  claimBlock: string | undefined;
  /** Whether the known claim block was proved final, not merely an older grant. */
  claimFinal: boolean;
  /** Whether the session still runs, independent of a post's own cancellation. */
  running(): boolean;
  /**
   * Where the allowance was read: at a best block that descends from the
   * claim's block, or at the finalized head. Usable at a best block is not
   * final.
   */
  usableAt: "best" | "finalized";
  /** The block the allowance was read at. */
  block: string;
  grant: NonNullable<Grant>;
  /** Why it waited for the finalized head, when it did. */
  why?: string;
  waitedMs: number;
  /**
   * Null once the allowance holds at the finalized head, or why it did not
   * within {@link FINALITY_TIMEOUT_MS}. Already null for `finalized`. Never
   * rejects.
   */
  final: Promise<string | null>;
}

export interface UsableAllowanceOptions extends FinalizedAllowanceOptions {
  rule: PostingRule;
  /** Read the account's allowance at a best block that descends from `claimBlock`. */
  readAfterChild(account: string, claimBlock: string): Promise<BestRead>;
}

/**
 * Wait until `claim`'s account may post by `options.rule`, or, with `wait`
 * `"finalized"`, until its allowance holds at the finalized head whatever
 * the rule. Rejects as {@link untilFinalizedAllowance} does, when the
 * finalized wait gives up or the session ends.
 */
export async function untilUsableAllowance(
  claim: ClaimedAccount,
  options: UsableAllowanceOptions,
  wait?: "finalized",
): Promise<UsableAllowance> {
  if (!options.running())
    throw new Error(
      "The session ended before the chat's allowance was usable.",
    );
  options = { ...options, requireClaimFinality: claim.blockHash !== undefined };
  const start = options.now();
  const atFinalized = async (why: string): Promise<UsableAllowance> => {
    const settled = await untilFinalizedAllowance(claim, options);
    return {
      account: claim.account,
      seq: claim.seq,
      collection: claim.collection,
      claimBlock: claim.blockHash,
      claimFinal: settled.claimFinal,
      running: options.running,
      usableAt: "finalized",
      block: settled.finalizedBlock,
      grant: settled.grant,
      why,
      waitedMs: options.now() - start,
      final: Promise.resolve(null),
    };
  };
  if (wait === "finalized")
    return atFinalized("the finalized head was asked for");
  if (options.rule.wait === "finalized") return atFinalized(options.rule.why);
  if (claim.blockHash === undefined)
    return atFinalized(
      "the account was registered before, in a block this host was not told",
    );
  let read: BestRead;
  try {
    read = await options.readAfterChild(claim.account, claim.blockHash);
  } catch (error) {
    return atFinalized(
      `the best block could not be read: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  if (!options.running())
    throw new Error(
      "The session ended before the chat's allowance was usable.",
    );
  if (read.tag === "final") return atFinalized("its claim is final already");
  if (read.tag === "unseen")
    return atFinalized(
      `its claim's block ${claim.blockHash} had no child at the best block within ${CHILD_TIMEOUT_MS / 1000} s`,
    );
  if (!isUsableGrant(read.grant))
    return atFinalized(
      `best block ${read.block} after its claim holds no allowance for it`,
    );
  const final = finalizedOrWhyNot(claim, options).then((outcome) =>
    "settled" in outcome
      ? null
      : outcome.ended
        ? "the session ended before its allowance was final"
        : `its allowance was not final after ${Math.round((options.now() - start) / 1000)} s: ${outcome.why}`,
  );
  return {
    account: claim.account,
    seq: claim.seq,
    collection: claim.collection,
    claimBlock: claim.blockHash,
    claimFinal: false,
    running: options.running,
    usableAt: "best",
    block: read.block,
    grant: read.grant,
    waitedMs: options.now() - start,
    final,
  };
}

/** What a caller asks of a guard beyond the network's rule. */
export interface GuardRequest {
  /** Wait for the finalized head whatever the rule. */
  wait?: "finalized";
  /** How a finalized wait that gives up names the account and the next step. */
  wording?: { account: string; retry: string };
  /**
   * Aborts when the session the claim was made for ends: the wait, and the
   * check behind a post, stop at once and read no more.
   */
  signal?: AbortSignal;
}

/** Wait until a claimed account may post, by the network's posting rule. */
export type AllowanceGuard = (
  claim: ClaimedAccount,
  request?: GuardRequest,
) => Promise<UsableAllowance>;

/**
 * The guard of the People chain behind `people`, by the rule `rule` answers
 * when asked. Logs where each account became usable, and a best-block
 * account whose claim did not become final.
 */
export function peopleAllowanceGuard(deps: {
  people(): PeopleAccess | null;
  rule(): PostingRule;
  log(line: string): void;
}): AllowanceGuard {
  const connected = (): PeopleAccess => {
    const access = deps.people();
    if (access === null) throw new Error("the People chain is not connected");
    return access;
  };
  return async (claim, request = {}) => {
    const { signal } = request;
    const usable = await untilUsableAllowance(
      claim,
      {
        rule: deps.rule(),
        read: async (account) => {
          if (claim.blockHash !== undefined)
            return readClaimGrant(
              connected(),
              account,
              claim.blockHash,
              signal,
            );
          const { block, values } = await readGrants(
            connected(),
            [account],
            signal,
          );
          return { block, grant: values[0]?.value ?? null };
        },
        readAfterChild: (account, claimBlock) =>
          readGrantAfterChild(
            connected(),
            account,
            claimBlock,
            CHILD_TIMEOUT_MS,
            signal,
          ),
        sleep: (ms) => sleepUnlessAborted(ms, signal),
        now: Date.now,
        running: () => deps.people() !== null && signal?.aborted !== true,
        ...(request.wording === undefined ? {} : { wording: request.wording }),
      },
      request.wait,
    );
    const short = `${claim.account.slice(0, 10)}…`;
    deps.log(
      usable.usableAt === "best"
        ? `Allowance: account ${short} may post at best block ${usable.block}, a child of its claim's block ${usable.claimBlock}, ${Math.round(usable.waitedMs / 1000)} s after the claim; its finality is checked behind the post.`
        : `Allowance: account ${short} holds ${usable.grant.maxCount} statements / ${usable.grant.maxSize} B at finalized block ${usable.block}, ${Math.round(usable.waitedMs / 1000)} s after the claim (${usable.why}).`,
    );
    if (usable.usableAt === "best")
      void usable.final.then((why) => {
        if (why !== null && signal?.aborted !== true)
          deps.log(
            `Allowance: account ${short} was usable at best block ${usable.block}, but ${why}.`,
          );
      });
    return usable;
  };
}

/** Wait `ms`, or less once `signal` aborts. */
function sleepUnlessAborted(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted === true) return resolve();
    const done = () => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", done);
      resolve();
    };
    const timer = setTimeout(done, ms);
    signal?.addEventListener("abort", done, { once: true });
  });
}
