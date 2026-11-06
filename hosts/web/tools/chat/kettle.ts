import { asProductId } from "@parity/truapi";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import type { ChatProductSpec } from "../../src/invitations/wallet-chat.js";
import {
  KETTLE_CHAT_PATH,
  irohParams,
  type IrohRelays,
} from "../../src/chat/kettle-chat/route.js";
import {
  readGrants,
  type Grant,
  type PeopleAccess,
} from "../statements/actions.js";

/**
 * PROTOTYPE. Kettle Chat (`src/chat/kettle-chat/`) as the host runs it when
 * the Chat tab (`./products.ts`) turns it on: its product, and the guard that
 * holds a conversation account's first post until its claim is final.
 */

/**
 * Kettle Chat, a chat product of its own: its own product id, worker,
 * request lists, storage and accounts. `settled` holds its first post per
 * conversation account until the account's claim is final. `iroh`
 * passes its worker the relays of Kettle's log transfer; null runs none.
 */
export function kettleChat(
  settled?: (claim: StatementAccountClaim) => Promise<void>,
  iroh: IrohRelays | null = null,
): ChatProductSpec {
  return {
    product: (networkSuffix) => ({
      id: asProductId(`kettle-chat.${networkSuffix}`),
      name: "Kettle Chat",
    }),
    workerPath: KETTLE_CHAT_PATH,
    lists: "kettle-chat",
    messagesEncrypted: true,
    ...(iroh === null ? {} : { workerParams: irohParams(iroh) }),
    ...(settled === undefined ? {} : { allowanceSettled: settled }),
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
}

/** How long a claim may take to finalize before the chat gives up. */
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
        `This chat's account ${claim.account} has its slot (${claim.collection} ${claim.seq}${claim.blockHash === undefined ? "" : `, block ${claim.blockHash}`}), but it was not final after ${Math.round(timeoutMs / 1000)} s: ${last}. Nothing was posted; send again to retry.`,
      );
    await options.sleep(options.intervalMs ?? FINALITY_INTERVAL_MS);
  }
}

/**
 * The guard a signed-in wallet's Kettle Chat uses: each conversation account
 * posts only once its claim holds at the finalized head. What it saw is
 * logged and kept on `window.__kettleAllowances`, for a test to read.
 */
export function finalizedAllowance(
  people: () => PeopleAccess | null,
  log: (line: string) => void,
): (claim: StatementAccountClaim) => Promise<void> {
  const seen: (SettledAllowance & { at: string })[] = [];
  (
    window as unknown as { __kettleAllowances: typeof seen }
  ).__kettleAllowances = seen;
  return async (claim) => {
    const settled = await untilFinalizedAllowance(claim, {
      read: async (account) => {
        const access = people();
        if (access === null)
          throw new Error("the People chain is not connected");
        const { block, values } = await readGrants(access, [account]);
        return { block, grant: values[0]?.value ?? null };
      },
      sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
      now: Date.now,
      running: () => people() !== null,
    });
    seen.push({ ...settled, at: new Date().toISOString() });
    log(
      `Kettle Chat: account ${claim.account.slice(0, 10)}… holds ${settled.grant.maxCount} statements / ${settled.grant.maxSize} B at finalized block ${settled.finalizedBlock} (claim ${claim.collection} slot ${claim.seq}${claim.blockHash === undefined ? "" : ` in block ${claim.blockHash}`}), ${Math.round(settled.waitedMs / 1000)} s after the claim's answer; it may post now.`,
    );
  };
}
