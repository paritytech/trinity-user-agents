import { asProductId } from "@parity/truapi";
import type { ChatProductSpec } from "../../src/invitations/wallet-chat.js";
import type { AllowanceSeen } from "../../src/invitations/wallet-delivery.js";
import { NOVA_CHAT_PATH } from "../../apps/nova-chat/route.js";
import { startNovaChat } from "../../src/integrations/nova-chat/start.js";

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
 * PROTOTYPE. Nova Chat (`apps/nova-chat/`) as the host runs it when the
 * Chat tab (`./products.ts`) turns it on: a chat product that talks to the
 * official Nova apps' chat, set up by the host's Nova integration
 * (`src/integrations/nova-chat/`). `seen` is told where each of its notification
 * and conversation accounts became usable; null: nobody is.
 */
export function novaChat(seen: AllowanceSeen | null): ChatProductSpec {
  return {
    ...NOVA_CHAT_FACTS,
    workerPath: NOVA_CHAT_PATH,
    lists: "nova-chat",
    start: startNovaChat,
    ...(seen === null ? {} : { allowanceSeen: seen }),
  };
}
