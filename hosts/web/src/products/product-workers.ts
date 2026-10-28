import type { TrUApiProductProvider } from "@parity/truapi-host";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { FrameTap } from "./tap.js";
import type { ActivitySink } from "../platform/activity.js";
import type { NetworkConfig } from "../platform/network-config.js";
import { openProduct, type OpenProduct } from "./product.js";
import { archiveCacheName, fileKey } from "./loader/files.js";
import { loaderUrl, mountScope } from "./loader/mount.js";
import { WORKER_SHELL, archivePath, contentTypeOf } from "./loader/serve.js";
import type { ResolvedWorker, WorkerManifest } from "./worker-manifest.js";
import type { WorkerConsoleEntry } from "./worker-console.js";

/**
 * The product workers the user added, run while their wallet is signed in.
 *
 * A worker is added by its product name and found at `worker.<name>`. Its
 * archive is mounted like a product opened by name, and it runs in a hidden
 * frame whose page loads the manifest's entry module, connected to the core
 * as the product's `Worker` execution. The product id is the name, the one
 * its app opens under, so the two share the product's storage and accounts.
 *
 * Pocket can hold a product's worker too, to draw a card. A product has one
 * worker whatever holds it, and it runs while anything does.
 *
 * The list is kept per network and wallet. Signing out, switching wallet or
 * network, or a reload stops every worker; signing in starts the saved ones.
 * A worker runs on this host's origin, as a product opened by name does, so
 * it is trusted like one (see the README's trust model).
 */

export type WorkerState =
  | { state: "starting" }
  | {
      state: "running";
      manifest: WorkerManifest;
      /** The record the worker was read from, such as `worker.myapp.paseo`. */
      record: string;
      cid: string;
    }
  | { state: "failed"; reason: string };

/** What a worker runs for: Chat's list of workers, or a card in Pocket. */
export type WorkerSurface = "chat" | "pocket";

/** One worker as the developer tools see it. */
export interface WorkerSnapshot {
  name: string;
  /**
   * Which start this is, numbered across the tab, so lines of a run before a
   * restart are told apart. Null for a worker that has not started.
   */
  run: number | null;
  /** The worker's own state, whatever any surface makes of it. Null when it has not started. */
  state: WorkerState | null;
  /** When the state last changed. */
  since: number | null;
  /** What wants it to run. */
  surfaces: WorkerSurface[];
  /** The host runs it for every wallet; the user cannot remove it. */
  addedByHost: boolean;
  /** Why the worker it runs does not serve a surface that wants it. */
  refusals: { surface: WorkerSurface; reason: string }[];
  /**
   * Whether {@link ProductWorkers.restart} may be offered for it: the user
   * added it, and nothing else holds it, so a new frame changes nothing the
   * host relies on.
   */
  restartable: boolean;
}

/** What the developer tools learn of the workers as they run. */
export interface WorkerObserver {
  /** The session changed: what was seen of the last one's workers is stale. */
  sessionChanged(): void;
  /** One run of a worker wrote `entry` to its console. */
  console(
    worker: { name: string; run: number },
    entry: WorkerConsoleEntry,
  ): void;
}

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
  activity: ActivitySink;
  /**
   * Workers the host adds for every wallet on a network, before any the user
   * adds, as the native app places a product's room by default. They run
   * like any other worker and cannot be removed.
   */
  defaults?(network: NetworkConfig): readonly string[];
  /**
   * The host end of the Chat Extension v2 draft bridge. A chat worker's frame
   * may hand it a port; the link ends when the worker stops.
   */
  chatV2?: {
    connect(productId: string, port: MessagePort): void;
    disconnect(productId: string): void;
  };
  /**
   * Where a chat worker's frame asks for the wallet's Nova chat identity
   * (`tools/demos/worker-mode.ts`), which decides whether to answer. The
   * link ends when the worker stops.
   */
  novaIdentity?: {
    connect(productId: string, port: MessagePort): void;
    disconnect(productId: string): void;
  };
  observer?: WorkerObserver;
}

interface Running {
  generation: number;
  run: number;
  state: WorkerState;
  since: number;
  handle: OpenProduct | null;
  cid: string | null;
  images: Map<string, string>;
  /** Why the worker does not serve a surface that wants it. */
  refusals: Map<WorkerSurface, string>;
  /** Settles once the start that made this entry has run or failed. */
  ready: Promise<void>;
}

/** The inspector channel a worker's frames are traced under. */
export const workerChannel = (name: string): string => `${name} (worker)`;

export const workersKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:workers:${walletId}:v1`;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class ProductWorkers {
  private session: WorkerSession | null = null;
  private generation = 0;
  private runs = 0;
  private readonly workers = new Map<string, Running>();
  /** Products Pocket holds a worker for, with the cards it draws. */
  private readonly pocketCards = new Map<string, string[]>();
  private readonly listeners = new Set<() => void>();

  constructor(private readonly deps: ProductWorkersDeps) {}

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of this.listeners) listener();
  }

  /** The host's default workers, then the names added for the signed-in wallet, in the order they were added. */
  names(): string[] {
    if (this.session === null) return [];
    return [...new Set([...this.defaultNames(), ...this.savedNames()])];
  }

  /** Whether the host adds `name` by default, so the user cannot remove it. */
  isDefault(name: string): boolean {
    return this.defaultNames().includes(name);
  }

  private defaultNames(): readonly string[] {
    return this.session === null
      ? []
      : (this.deps.defaults?.(this.session.network) ?? []);
  }

  private savedNames(): string[] {
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

  /**
   * The worker's state as `surface` sees it: failed with the reason when the
   * worker runs for another surface but does not serve this one.
   */
  state(name: string, surface: WorkerSurface = "chat"): WorkerState | null {
    const running = this.workers.get(name);
    if (running === undefined) return null;
    const refusal = running.refusals.get(surface);
    return refusal === undefined
      ? running.state
      : { state: "failed", reason: refusal };
  }

  /** Whether `name`'s worker runs and serves `surface`. */
  serves(name: string, surface: WorkerSurface): boolean {
    const running = this.workers.get(name);
    return (
      running?.state.state === "running" &&
      this.wanted(name).has(surface) &&
      !running.refusals.has(surface)
    );
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
   * ones of the new session, if any. Pocket's holds belong to the session
   * and end with it.
   */
  setSession(session: WorkerSession | null): void {
    if (
      session?.walletId === this.session?.walletId &&
      session?.network.id === this.session?.network.id
    )
      return;
    this.stopAll();
    this.session = session;
    this.deps.observer?.sessionChanged();
    for (const name of this.names()) void this.start(name);
    this.changed();
  }

  /** Start `name`'s worker again in a new frame, if this session wants it. */
  restart(name: string): Promise<void> {
    return this.wanted(name).size === 0 ? Promise.resolve() : this.start(name);
  }

  /** Start the wanted workers not running yet, as once the core is up. */
  resume(): void {
    for (const name of new Set([...this.names(), ...this.pocketCards.keys()]))
      if (!this.workers.has(name)) void this.start(name);
  }

  /** Add `name`'s worker for this wallet and start it. Resolves once it runs; throws when it cannot. */
  async add(name: string): Promise<void> {
    if (this.session === null) throw new Error("Sign in to add a worker.");
    const trimmed = name.trim().toLowerCase();
    if (trimmed === "") throw new Error("Enter a product name.");
    if (!this.names().includes(trimmed))
      this.saveNames([...this.savedNames(), trimmed]);
    await this.settle(trimmed);
    const state = this.state(trimmed);
    if (state?.state === "failed") throw new Error(state.reason);
  }

  /** Take `name` off this wallet's list, and stop its worker unless Pocket still holds it. */
  remove(name: string): void {
    this.saveNames(this.savedNames().filter((entry) => entry !== name));
    void this.settle(name);
  }

  /**
   * Run `name`'s worker for Pocket while this session lasts, to draw
   * `cardIds`. A worker that does not serve Pocket, or does not draw one of
   * the cards, is not started for it.
   */
  holdForPocket(name: string, cardIds: string[]): Promise<void> {
    if (this.session === null) return Promise.resolve();
    this.pocketCards.set(name, cardIds);
    return this.settle(name);
  }

  /** Start or stop workers after the host's defaults changed. */
  defaultsChanged(): void {
    for (const name of new Set([...this.workers.keys(), ...this.names()]))
      void this.settle(name);
  }

  /** Stop `name`'s worker for Pocket; it keeps running if Chat lists it. */
  releaseForPocket(name: string): void {
    if (!this.pocketCards.delete(name)) return;
    void this.settle(name);
  }

  /** The surfaces that want `name`'s worker. */
  private wanted(name: string): Set<WorkerSurface> {
    const wanted = new Set<WorkerSurface>();
    if (this.names().includes(name)) wanted.add("chat");
    if (this.pocketCards.has(name)) wanted.add("pocket");
    return wanted;
  }

  /** Why the worker `record` publishes does not serve each wanted surface. */
  private refusals(
    name: string,
    record: string,
    manifest: WorkerManifest,
  ): Map<WorkerSurface, string> {
    const refusals = new Map<WorkerSurface, string>();
    const wanted = this.wanted(name);
    if (wanted.has("chat") && !manifest.includes.chat)
      refusals.set(
        "chat",
        `${record} serves no chat, so this host has nothing to show for it.`,
      );
    if (wanted.has("pocket")) {
      const missing = (this.pocketCards.get(name) ?? []).filter(
        (card) => !manifest.pocketCards.includes(card),
      );
      if (!manifest.includes.pocket)
        refusals.set("pocket", `${record} serves no Pocket.`);
      else if (missing.length > 0)
        refusals.set(
          "pocket",
          `${record} draws no ${missing.join(", ")} card in Pocket.`,
        );
    }
    return refusals;
  }

  /**
   * Bring `name`'s worker in line with what wants it: stop it when nothing
   * does, start it when it is not running, and otherwise read again which
   * surfaces it serves.
   */
  private async settle(name: string): Promise<void> {
    const wanted = this.wanted(name);
    const running = this.workers.get(name);
    if (wanted.size === 0) {
      this.stop(name);
      this.workers.delete(name);
      this.changed();
      return;
    }
    if (running === undefined || running.state.state === "failed")
      return this.start(name);
    if (running.state.state === "starting") return running.ready;
    running.refusals = this.refusals(
      name,
      running.state.record,
      running.state.manifest,
    );
    if ([...wanted].every((surface) => running.refusals.has(surface))) {
      const [reason] = running.refusals.values();
      this.stop(name);
      this.setState(running, { state: "failed", reason: reason! });
    }
    this.changed();
  }

  private start(name: string): Promise<void> {
    const session = this.session;
    const runtime = this.deps.runtime();
    if (session === null || runtime === null) return Promise.resolve();
    this.stop(name);
    const running: Running = {
      generation: this.generation,
      run: (this.runs += 1),
      state: { state: "starting" },
      since: Date.now(),
      handle: null,
      cid: null,
      images: new Map(),
      refusals: new Map(),
      ready: Promise.resolve(),
    };
    this.workers.set(name, running);
    this.changed();
    running.ready = this.run(name, running, session, runtime);
    return running.ready;
  }

  private async run(
    name: string,
    running: Running,
    session: WorkerSession,
    runtime: WorkerPairingHostRuntime,
  ): Promise<void> {
    const current = () =>
      this.workers.get(name) === running &&
      running.generation === this.generation;
    try {
      const worker = await this.deps.resolve(name, session.network);
      if (!current()) return;
      running.refusals = this.refusals(name, worker.record, worker.manifest);
      const wanted = this.wanted(name);
      if ([...wanted].every((surface) => running.refusals.has(surface))) {
        const [reason] = running.refusals.values();
        throw new Error(reason);
      }
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
        gateway: worker.gateway ?? session.network.dotns.contentGateway,
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
          tap: this.deps.tap(workerChannel(name)),
          ...(this.deps.chatV2 !== undefined && worker.manifest.includes.chat
            ? {
                onChatV2Port: (port: MessagePort) => {
                  if (current()) this.deps.chatV2?.connect(name, port);
                  else port.close();
                },
              }
            : {}),
          ...(this.deps.novaIdentity !== undefined &&
          worker.manifest.includes.chat
            ? {
                onNovaIdentityPort: (port: MessagePort) => {
                  if (current()) this.deps.novaIdentity?.connect(name, port);
                  else port.close();
                },
              }
            : {}),
          ...(this.deps.observer === undefined
            ? {}
            : {
                onConsole: (entry: WorkerConsoleEntry) => {
                  if (current())
                    this.deps.observer?.console(
                      { name, run: running.run },
                      entry,
                    );
                },
              }),
          onSandboxStatus: (state, detail) => {
            if (state === "error" && current()) {
              this.setState(running, { state: "failed", reason: detail });
              this.failed(name, detail);
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
        this.setState(running, {
          state: "running",
          manifest: worker.manifest,
          record: worker.record,
          cid: worker.cid,
        });
      this.deps.activity({
        category: "workers",
        action: "Started",
        subject: name,
        details: [
          { label: "Record", value: worker.record },
          { label: "Content", value: worker.cid },
          { label: "Entry", value: worker.manifest.entrypoint },
        ],
      });
    } catch (error) {
      if (!current()) return;
      running.refusals = new Map();
      this.setState(running, { state: "failed", reason: errorText(error) });
      this.failed(name, errorText(error));
    }
    this.changed();
  }

  private setState(running: Running, state: WorkerState): void {
    running.state = state;
    running.since = Date.now();
  }

  /**
   * Every worker this session wants or runs, in the order the menu lists
   * them, then those only Pocket holds.
   */
  inspect(): WorkerSnapshot[] {
    const names = [
      ...new Set([
        ...this.names(),
        ...this.pocketCards.keys(),
        ...this.workers.keys(),
      ]),
    ];
    return names.map((name) => {
      const running = this.workers.get(name);
      const surfaces = [...this.wanted(name)];
      const addedByHost = this.isDefault(name);
      return {
        name,
        run: running?.run ?? null,
        state: running?.state ?? null,
        since: running?.since ?? null,
        surfaces,
        addedByHost,
        refusals: [...(running?.refusals ?? new Map())].map(
          ([surface, reason]) => ({ surface, reason }),
        ),
        restartable:
          !addedByHost &&
          surfaces.length === 1 &&
          surfaces[0] === "chat" &&
          running !== undefined &&
          running.state.state !== "starting",
      };
    });
  }

  private failed(name: string, reason: string): void {
    this.deps.activity({
      category: "workers",
      action: "Failed",
      subject: name,
      note: reason,
      failed: true,
    });
  }

  private stop(name: string): void {
    const running = this.workers.get(name);
    if (running === undefined) return;
    this.deps.chatV2?.disconnect(name);
    this.deps.novaIdentity?.disconnect(name);
    if (running.handle !== null) {
      this.deps.runtime()?.releaseWorker(name);
      running.handle.dispose();
      running.handle = null;
      this.deps.activity({
        category: "workers",
        action: "Stopped",
        subject: name,
      });
    }
    for (const url of running.images.values()) URL.revokeObjectURL(url);
    running.images.clear();
  }

  /** Stop every worker, as at sign-out. Nothing is removed from the saved list. */
  stopAll(): void {
    this.generation += 1;
    for (const name of this.workers.keys()) this.stop(name);
    this.workers.clear();
    this.pocketCards.clear();
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
