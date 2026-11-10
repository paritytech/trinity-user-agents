/**
 * Where the host serves Nova Chat's worker: a description of its app archive
 * at {@link NOVA_CHAT_PATH} under the host's base, as for Contact Chat
 * (`../contact-chat/route.ts`). Free of other imports so the Vite config can
 * load it.
 */

/** Nova Chat's worker description, under the host's base. */
export const NOVA_CHAT_PATH = "nova-chat/worker.json";
