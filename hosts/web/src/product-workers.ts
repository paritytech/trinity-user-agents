import type { TrUApiProductProvider } from "@parity/truapi-host";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { FrameTap } from "./frame-tap.js";
import type { NetworkConfig } from "./network-config.js";
import { openProduct, type OpenProduct } from "./product.js";
import { archiveCacheName, fileKey } from "./sandbox/files.js";
import { loaderUrl, mountScope } from "./sandbox/mount.js";
import { WORKER_SHELL, archivePath, contentTypeOf } from "./sandbox/serve.js";
import type { ResolvedWorker } from "./worker-manifest.js";

/**
 * The product workers the user added, run while their wallet is signed in.
 *
 * A worker is added by its product name and found at `worker.<name>`. Its
 * archive is mounted like a product opened by name, and it runs in a hidden
 * frame whose page loads the manifest's entry module, connected to the core
 * as the product's `Worker` execution. The product id is the name, the one
 * its app opens under, so the two share the product's storage and accounts.
 *
 * The list is kept per network and wallet. Signing out, switching wallet or
 * network, or a reload stops every worker; signing in starts the saved ones.
 * A worker runs on this host's origin, as a product opened by name does, so
 * it is trusted like one (see the README's trust model).
 */

export type WorkerState =
  | { state: "starting" }
  | { state: "running"; manifest: ResolvedWorker["manifest"]; cid: string }
  | { state: "failed"; reason: string };

export interface WorkerSession {
  walletId: string;
  network: NetworkConfig;
}

export interface ProductWorkersDeps {
  runtime(): WorkerPairingHostRuntime | null;
  resolve(name: string, network: NetworkConfig): Promise<ResolvedWorker>;
  /** Where the hidden worker frames are put. */
  container: HTMLElement;
  /** The host's base path, as the mounts are built from. */
  base: string;
  storage: Storage;
  tap(channel: string): FrameTap | undefined;
  log(text: string): void;
}

interface Running {
  generation: number;
  state: WorkerState;
  handle: OpenProduct | null;
  cid: string | null;
  images: Map<string, string>;
}

export const workersKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:workers:${walletId}:v1`;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class ProductWorkers {
  private session: WorkerSession | null = null;
  private generation = 0;
  private readonly workers = new Map<string, Running>();
  private readonly listeners = new Set<() => void>();

  constructor(private readonly deps: ProductWorkersDeps) {}

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of this.listeners) listener();
  }

  /** The names added for the signed-in wallet, in the order they were added. */
  names(): string[] {
    if (this.session === null) return [];
    try {
      const saved: unknown = JSON.parse(
        this.deps.storage.getItem(
          workersKey(this.session.network.id, this.session.walletId),
        ) ?? "[]",
      );
      return Array.isArray(saved)
        ? saved.filter((name): name is string => typeof name === "string")
        : [];
    } catch {
      return [];
    }
  }

  private saveNames(names: string[]): void {
    if (this.session === null) return;
    this.deps.storage.setItem(
      workersKey(this.session.network.id, this.session.walletId),
      JSON.stringify(names),
    );
  }

  state(name: string): WorkerState | null {
    return this.workers.get(name)?.state ?? null;
  }

  /** The running worker's channel to the core, or null. */
  provider(productId: string): TrUApiProductProvider | null {
    const running = this.workers.get(productId);
    return running?.state.state === "running"
      ? (running.handle?.provider ?? null)
      : null;
  }

  /**
   * Follow the signed-in session: stop every worker, then start the saved
   * ones of the new session, if any.
   */
  setSession(session: WorkerSession | null): void {
    if (
      session?.walletId === this.session?.walletId &&
      session?.network.id === this.session?.network.id
    )
      return;
    this.stopAll();
    this.session = session;
    for (const name of this.names()) void this.start(name);
    this.changed();
  }

  /** Start the saved workers not running yet, as once the core is up. */
  resume(): void {
    for (const name of this.names())
      if (!this.workers.has(name)) void this.start(name);
  }

  /** Add `name`'s worker for this wallet and start it. Resolves once it runs; throws when it cannot. */
  async add(name: string): Promise<void> {
    if (this.session === null) throw new Error("Sign in to add a worker.");
    const trimmed = name.trim().toLowerCase();
    if (trimmed === "") throw new Error("Enter a product name.");
    if (!this.names().includes(trimmed))
      this.saveNames([...this.names(), trimmed]);
    await this.start(trimmed);
    const state = this.state(trimmed);
    if (state?.state === "failed") throw new Error(state.reason);
  }

  /** Stop `name`'s worker and remove it from this wallet's list. */
  remove(name: string): void {
    this.stop(name);
    this.workers.delete(name);
    this.saveNames(this.names().filter((entry) => entry !== name));
    this.changed();
  }

  private async start(name: string): Promise<void> {
    const session = this.session;
    const runtime = this.deps.runtime();
    if (session === null || runtime === null) return;
    this.stop(name);
    const running: Running = {
      generation: this.generation,
      state: { state: "starting" },
      handle: null,
      cid: null,
      images: new Map(),
    };
    this.workers.set(name, running);
    this.changed();
    const current = () =>
      this.workers.get(name) === running &&
      running.generation === this.generation;
    try {
      const worker = await this.deps.resolve(name, session.network);
      if (!current()) return;
      if (!worker.manifest.includes.chat)
        throw new Error(
          `${worker.record} serves no chat, so this host has nothing to show for it.`,
        );
      const scope = mountScope(this.deps.base, {
        walletId: session.walletId,
        productId: name,
        cid: worker.cid,
        network: session.network.id,
      });
      const url = loaderUrl({
        base: this.deps.base,
        origin: window.location.origin,
        scope,
        gateway: session.network.dotns.contentGateway,
        kind: worker.kind,
        start: `/${WORKER_SHELL}?entry=${encodeURIComponent(worker.manifest.entrypoint)}`,
        entry: worker.manifest.entrypoint,
      });
      const handle = await openProduct(
        runtime,
        url,
        name,
        this.deps.container,
        {
          mounted: true,
          executionKind: "Worker",
          tap: this.deps.tap(`${name} (worker)`),
          onSandboxStatus: (state, detail) => {
            if (state === "error" && current()) {
              running.state = { state: "failed", reason: detail };
              this.changed();
            }
          },
        },
      );
      if (!current()) {
        handle.dispose();
        return;
      }
      handle.setVisible(false);
      runtime.acquireWorker(name);
      running.handle = handle;
      running.cid = worker.cid;
      if (running.state.state === "starting")
        running.state = {
          state: "running",
          manifest: worker.manifest,
          cid: worker.cid,
        };
      this.deps.log(`worker ${name} started from ${worker.record}`);
    } catch (error) {
      if (!current()) return;
      running.state = { state: "failed", reason: errorText(error) };
      this.deps.log(`worker ${name}: ${errorText(error)}`);
    }
    this.changed();
  }

  private stop(name: string): void {
    const running = this.workers.get(name);
    if (running === undefined) return;
    if (running.handle !== null) {
      this.deps.runtime()?.releaseWorker(name);
      running.handle.dispose();
      running.handle = null;
    }
    for (const url of running.images.values()) URL.revokeObjectURL(url);
    running.images.clear();
  }

  /** Stop every worker, as at sign-out. Nothing is removed from the saved list. */
  stopAll(): void {
    this.generation += 1;
    for (const name of this.workers.keys()) this.stop(name);
    this.workers.clear();
    this.changed();
  }

  /**
   * A URL for a file of `productId`'s worker archive, or null when there is
   * no such file. It is read from the copy the loader checked and stored in
   * the worker frame's own storage, which the worker itself can also write.
   */
  async archiveImage(productId: string, path: string): Promise<string | null> {
    const running = this.workers.get(productId);
    if (running?.cid == null || archivePath(`/${path}`) !== path) return null;
    const known = running.images.get(path);
    if (known !== undefined) return known;
    const type = contentTypeOf(path);
    if (!type.startsWith("image/")) return null;
    const stored = await running.handle
      ?.frameCaches()
      ?.match(fileKey(window.location.origin, path), {
        cacheName: archiveCacheName(running.cid),
      });
    if (stored === undefined || this.workers.get(productId) !== running)
      return null;
    const url = URL.createObjectURL(
      new Blob([await stored.arrayBuffer()], { type }),
    );
    running.images.set(path, url);
    return url;
  }
}
