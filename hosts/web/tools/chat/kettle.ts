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
  untilFinalizedAllowance,
  type PeopleAccess,
  type SettledAllowance,
} from "../../src/platform/allowance-finality.js";

/**
 * PROTOTYPE. Kettle Chat (`src/chat/kettle-chat/`) as the host runs it when
 * the Chat tab (`./products.ts`) turns it on: its product, and the guard that
 * holds a conversation account's first post until its claim is final.
 */

/** Kettle Chat's product, and whether it encrypts its messages, as the Chat tab shows them. */
export const KETTLE_CHAT_FACTS: Pick<
  ChatProductSpec,
  "product" | "messagesEncrypted"
> = {
  product: (networkSuffix) => ({
    id: asProductId(`kettle-chat.${networkSuffix}`),
    name: "Kettle Chat",
  }),
  messagesEncrypted: true,
};

/**
 * Kettle Chat, a chat product of its own: its own product id, worker,
 * request lists, storage and accounts. `settled` holds its first post per
 * conversation account until the account's claim is final. `iroh`
 * passes its worker the relays of Kettle's log transfer; null runs none.
 */
export function kettleChat(
  settled: (claim: StatementAccountClaim) => Promise<void>,
  iroh: IrohRelays | null = null,
): ChatProductSpec {
  return {
    ...KETTLE_CHAT_FACTS,
    workerPath: KETTLE_CHAT_PATH,
    lists: "kettle-chat",
    ...(iroh === null ? {} : { workerParams: irohParams(iroh) }),
    allowanceSettled: settled,
  };
}

/** Who uses a {@link finalizedAllowance} guard: its log's name, where it keeps what it saw, and its timeout's wording. */
export interface AllowanceGuardLabel {
  product: string;
  /** The `window` property that keeps what the guard saw, for a test to read. */
  record: string;
  wording?: { account: string; retry: string };
}

/** Kettle Chat's label, as its live test reads it. */
export const KETTLE_ALLOWANCES: AllowanceGuardLabel = {
  product: "Kettle Chat",
  record: "__kettleAllowances",
};

/**
 * The guard a signed-in wallet's chat product uses: an account posts only
 * once its claim holds at the finalized head. What it saw is logged and kept
 * on `window[label.record]`, for a test to read.
 */
export function finalizedAllowance(
  people: () => PeopleAccess | null,
  log: (line: string) => void,
  label: AllowanceGuardLabel = KETTLE_ALLOWANCES,
): (claim: StatementAccountClaim) => Promise<void> {
  const seen: (SettledAllowance & { at: string })[] = [];
  (window as unknown as Record<string, typeof seen>)[label.record] = seen;
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
      ...(label.wording === undefined ? {} : { wording: label.wording }),
    });
    seen.push({ ...settled, at: new Date().toISOString() });
    log(
      `${label.product}: account ${claim.account.slice(0, 10)}… holds ${settled.grant.maxCount} statements / ${settled.grant.maxSize} B at finalized block ${settled.finalizedBlock} (claim ${claim.collection} slot ${claim.seq}${claim.blockHash === undefined ? "" : ` in block ${claim.blockHash}`}), ${Math.round(settled.waitedMs / 1000)} s after the claim's answer; it may post now.`,
    );
  };
}
