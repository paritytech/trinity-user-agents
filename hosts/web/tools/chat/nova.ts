import { asProductId } from "@parity/truapi";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import type { ChatProductSpec } from "../../src/invitations/wallet-chat.js";
import { NOVA_CHAT_PATH } from "../../src/chat/nova-chat/route.js";

/** Nova Chat's product, and whether it encrypts its messages, as the Chat tab shows them. */
export const NOVA_CHAT_FACTS: Pick<
  ChatProductSpec,
  "product" | "messagesEncrypted"
> = {
  product: (networkSuffix) => ({
    id: asProductId(`nova-chat.${networkSuffix}`),
    name: "Nova Chat",
  }),
  messagesEncrypted: true,
};

/**
 * PROTOTYPE. Nova Chat (`src/chat/nova-chat/`) as the host runs it when the
 * Chat tab (`./products.ts`) turns it on: a chat product that talks to the
 * official Nova apps' chat. `settled` holds each notification account's
 * post, and a conversation account's first post, until its claim is final.
 */
export function novaChat(
  settled: (claim: StatementAccountClaim) => Promise<void>,
): ChatProductSpec {
  return {
    ...NOVA_CHAT_FACTS,
    workerPath: NOVA_CHAT_PATH,
    lists: "nova-chat",
    setup: "nova",
    allowanceSettled: settled,
  };
}
