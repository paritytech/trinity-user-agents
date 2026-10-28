/**
 * Where the dev server answers for the local chat product demo
 * (`chat-product-demo-plugin.ts`). Shared by the plugin and the page, and
 * free of other imports so the Vite config can load it.
 */

/** The demo worker's description, under the host's base. */
export const DEMO_WORKER_PATH = "__chat-product-demo/worker.json";

/** The Nova chat worker's description (`src/chat-worker/worker.ts`), under the host's base. */
export const NOVA_WORKER_PATH = "__chat-product-demo/nova-worker.json";

/** Example Chat's app archive (`fixtures/products/example-chat/product.ts`), under the host's base. */
export const EXAMPLE_CHAT_PATH = "__chat-product-demo/example-chat.json";

/** What the dev server answers at {@link DEMO_WORKER_PATH}. */
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
