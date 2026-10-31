/**
 * Where the dev server answers for the local chat product
 * (`chat-product-demo-plugin.ts`). Shared by the plugin and the page, and
 * free of other imports so the Vite config can load it.
 */

/** Contact Chat's worker (`fixtures/products/contact-chat/worker.ts`), under the host's base. */
export const CONTACT_CHAT_PATH = "__chat-product-demo/contact-chat-worker.json";

/** What the dev server answers at each product's path. */
export interface DemoWorkerAnswer {
  /** Where the content came from, for the screen. */
  record: string;
  /** The archive's CID, whose blocks the dev server serves at `/ipfs/<cid>`. */
  cid: string;
  kind: "car";
  /** The executable manifest, as a `worker.<name>` text record holds it. */
  manifest: string;
}

/** Host names the demo answers on: this machine only. */
export const LOOPBACK_HOSTNAMES = ["localhost", "127.0.0.1", "[::1]", "::1"];

/** Whether this page may offer the demos: a dev server on this machine. */
export function demoAllowed(dev: boolean, hostname: string): boolean {
  return dev && LOOPBACK_HOSTNAMES.includes(hostname);
}
