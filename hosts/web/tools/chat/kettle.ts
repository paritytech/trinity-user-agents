import { asProductId } from "@parity/truapi";
import type { AllowanceSeen } from "../../src/invitations/wallet-delivery.js";
import type { ChatProductSpec } from "../../src/invitations/wallet-chat.js";
import {
  KETTLE_CHAT_PATH,
  irohParams,
  type IrohRelays,
} from "../../src/chat/kettle-chat/route.js";

/**
 * PROTOTYPE. Kettle Chat (`src/chat/kettle-chat/`) as the host runs it when
 * the Chat tab (`./products.ts`) turns it on: its product, and a record of
 * where each of its accounts became usable, for its live test.
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
 * request lists, storage and accounts. `seen` is told where each of its
 * accounts became usable; null: nobody is. `iroh` passes its worker the
 * relays of Kettle's log transfer; null runs none.
 */
export function kettleChat(
  seen: AllowanceSeen | null,
  iroh: IrohRelays | null = null,
): ChatProductSpec {
  return {
    ...KETTLE_CHAT_FACTS,
    workerPath: KETTLE_CHAT_PATH,
    lists: "kettle-chat",
    ...(iroh === null ? {} : { workerParams: irohParams(iroh) }),
    ...(seen === null ? {} : { allowanceSeen: seen }),
  };
}

/**
 * Keep where each account became usable on `window[record]`, for a test to
 * read: `__kettleAllowances` for Kettle Chat, `__novaAllowances` for Nova
 * Chat. The guard (`src/platform/allowance-usable.ts`) decides; this only
 * records.
 */
export function recordAllowances(record: string): AllowanceSeen {
  const seen: Record<string, unknown>[] = [];
  (window as unknown as Record<string, typeof seen>)[record] = seen;
  return ({ final: _final, ...usable }) =>
    void seen.push({
      ...usable,
      ...(usable.usableAt === "finalized"
        ? { finalizedBlock: usable.block }
        : {}),
      at: new Date().toISOString(),
    });
}
