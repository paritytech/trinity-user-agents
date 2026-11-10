import { asProductId } from "@parity/truapi";
import { CONTACT_CHAT_PATH } from "../../apps/contact-chat/route.js";
import type { ChatProductSpec } from "../invitations/wallet-chat.js";

/**
 * The chat products this host bundles in every build: Contact Chat, which
 * runs for every signed-in wallet unless the developer tools' Chat tab chose
 * others. Kettle Chat and Nova Chat, for the dev server and a build with
 * `WEB_SIGNING_KETTLE_CHAT=1`, are named in `tools/chat/`.
 */

/** Contact Chat, the chat product every session runs. */
export const CONTACT_CHAT: ChatProductSpec = {
  product: (networkSuffix) => ({
    id: asProductId(`contact-chat.${networkSuffix}`),
    name: "Contact Chat",
  }),
  workerPath: CONTACT_CHAT_PATH,
  lists: "contact-chat",
  messagesEncrypted: false,
};
