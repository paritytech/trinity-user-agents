import { asProductId } from "@parity/truapi";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import type { ChatProductSpec } from "../../src/invitations/wallet-chat.js";
import { NOVA_CHAT_PATH } from "../../src/chat/nova-chat/route.js";

/**
 * PROTOTYPE. Nova Chat (`src/chat/nova-chat/`) as the host runs it when the
 * Chat tab (`./products.ts`) turns it on: a chat product that talks to the
 * official Nova apps' chat. `settled` holds a conversation account's first
 * post until its claim is final.
 */
export function novaChat(
  settled?: (claim: StatementAccountClaim) => Promise<void>,
): ChatProductSpec {
  return {
    product: (networkSuffix) => ({
      id: asProductId(`nova-chat.${networkSuffix}`),
      name: "Nova Chat",
    }),
    workerPath: NOVA_CHAT_PATH,
    lists: "nova-chat",
    messagesEncrypted: true,
    setup: "nova",
    ...(settled === undefined ? {} : { allowanceSettled: settled }),
  };
}
