/**
 * Where the host serves Contact Chat's worker: a description of its app
 * archive at {@link CONTACT_CHAT_PATH} under the host's base, and the
 * archive's blocks at `/ipfs/<cid>` on the host's origin, as a gateway serves
 * them. The dev server builds them on request and a build writes them beside
 * the page (`contact-chat-plugin.ts`). Free of other imports so the Vite
 * config can load it.
 */

/** Contact Chat's worker description, under the host's base. */
export const CONTACT_CHAT_PATH = "contact-chat/worker.json";

/** What the host serves at {@link CONTACT_CHAT_PATH}. */
export interface ContactChatWorkerAnswer {
  /** Where the content came from, for the screen. */
  record: string;
  /** The archive's CID, whose blocks the host serves at `/ipfs/<cid>`. */
  cid: string;
  kind: "car";
  /** The executable manifest, as a `worker.<name>` text record holds it. */
  manifest: string;
}
