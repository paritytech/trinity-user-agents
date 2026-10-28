import type { ResolvedWorker } from "../worker-manifest.js";
import { parseWorkerManifest } from "../worker-manifest.js";
import type { CallMediaEngine } from "./host.js";
import {
  DEMO_WORKER_PATH,
  LOOPBACK_HOSTNAMES,
  type DemoWorkerAnswer,
} from "./local-demo-route.js";
import { syntheticCallMedia } from "../../fixtures/products/synthetic-media.js";

/**
 * DEV ONLY. The local chat product demo, as the page runs it: the synthetic
 * chat product (`fixtures/products/synthetic-chat-product.ts`) as a product worker,
 * served by this dev server (`chat-product-demo-plugin.ts`).
 *
 * The user starts it for the signed-in session. It then runs as one of the
 * host's own workers, through the same loader, archive check and core as a
 * worker found by name, and its rooms show through the chat product provider.
 * It stops when the user stops it, signs out, or changes wallet or network.
 * Nothing is saved.
 *
 * Its product id is `localhost:<port>`, a development id the core accepts and
 * no published name can take. Its calls get the synthetic call engine, which
 * opens no camera or microphone; no other product does.
 */

export interface LocalChatDemoDeps {
  base: string;
  origin: string;
  port: string;
  /** Called when the demo is turned on or off, for the workers to follow. */
  changed(): void;
  fetch?: typeof fetch;
}

/** Whether this page may offer the demo: a dev server on this machine. */
export function demoAllowed(dev: boolean, hostname: string): boolean {
  return dev && LOOPBACK_HOSTNAMES.includes(hostname);
}

export class LocalChatDemo {
  readonly productId: string;
  /** Where the demo's code comes from, for the screen. */
  readonly source =
    "fixtures/products/synthetic-chat-product.ts, served by this dev server";
  private on = false;

  constructor(private readonly deps: LocalChatDemoDeps) {
    this.productId = `localhost:${deps.port === "" ? "80" : deps.port}`;
  }

  running(): boolean {
    return this.on;
  }

  /** The workers the host adds while the demo is on. */
  workerNames(): string[] {
    return this.on ? [this.productId] : [];
  }

  start(): void {
    if (this.on) return;
    this.on = true;
    this.deps.changed();
  }

  stop(): void {
    if (!this.on) return;
    this.on = false;
    this.deps.changed();
  }

  /** Whether `name` is the demo's worker, while the demo is on. */
  resolves(name: string): boolean {
    return this.on && name === this.productId;
  }

  /** The demo's worker, as the dev server describes the archive it built. */
  async resolve(): Promise<ResolvedWorker> {
    const response = await (this.deps.fetch ?? fetch)(
      `${this.deps.base}${DEMO_WORKER_PATH}`,
      { cache: "no-store" },
    );
    if (!response.ok)
      throw new Error(
        `The dev server did not build the chat product demo: ${await response.text()}`,
      );
    const answer = (await response.json()) as DemoWorkerAnswer;
    return {
      record: answer.record,
      cid: answer.cid,
      kind: answer.kind,
      manifest: parseWorkerManifest(answer.manifest),
      gateway: this.deps.origin,
    };
  }

  /** The call engine for `productId`'s calls: the synthetic one for the demo only. */
  media(productId: string): CallMediaEngine | null {
    return this.on && productId === this.productId ? syntheticCallMedia : null;
  }
}
