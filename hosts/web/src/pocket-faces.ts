import { bytesToHex } from "@parity/truapi/scale";
import type { HexString, RenderContext, RendererNode } from "@parity/truapi";
import type { TrUApiProductProvider } from "@parity/truapi-host";
import type { WorkerSurface } from "./product-workers.js";

/**
 * The faces of the cards in Pocket: the one kept from the last time, then
 * every face the card's product draws through its worker.
 *
 * A card is a product's (`productId`, `cardId`). Its worker draws the face on
 * the `PocketCard` render context, and presses inside the face go back on the
 * same context (docs/rfcs/pocket-modality.md). Until a face arrives the card
 * shows the face this host bundles for it, and no face is a reading of the
 * user's standing: it is whatever the product draws.
 *
 * The workers start the first time Pocket is shown in a session and run
 * until the session ends. The newest face of each card is kept per network
 * and wallet, at most every {@link KEEP_INTERVAL_MS} and once more when the
 * session ends, so it is shown at once next time.
 */

/** One card in Pocket. */
export interface PocketCardKey {
  productId: string;
  cardId: string;
}

/** Where the face on screen came from. */
export type FaceSource = "bundled" | "kept" | "live";

export interface PocketFace {
  /** The tree to draw, or null to draw the bundled face. */
  tree: RendererNode | null;
  source: FaceSource;
}

/** What Pocket needs of the product workers. */
export interface PocketWorkers {
  holdForPocket(name: string, cardIds: string[]): Promise<void>;
  serves(name: string, surface: WorkerSurface): boolean;
  provider(productId: string): TrUApiProductProvider | null;
  onChange(listener: () => void): () => void;
}

export interface PocketSession {
  walletId: string;
  networkId: string;
  cards: PocketCardKey[];
}

export interface PocketFacesDeps {
  workers: PocketWorkers;
  storage: Storage;
  log(text: string): void;
  now?(): number;
}

/** How long a worker that just started is asked again quickly: it is booting, not failing. */
export const CONNECT_WINDOW_MS = 10_000;
export const CONNECT_RETRY_MS = 250;
/** Reopening a face stream that ended doubles from a second up to half a minute. */
export const REOPEN_MS = 1_000;
export const MAX_REOPEN_MS = 30_000;
/** How often the newest face is written down. */
export const KEEP_INTERVAL_MS = 2_000;
/** A face larger than this, as JSON, is drawn but not kept. */
export const MAX_KEPT_BYTES = 256 * 1024;

export const pocketFacesKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:pocket-faces:${walletId}:v1`;

const cardId = (key: PocketCardKey): string => `${key.productId}/${key.cardId}`;

const context = (key: PocketCardKey): RenderContext => ({
  tag: "PocketCard",
  value: { cardId: key.cardId },
});

interface Stream {
  provider: TrUApiProductProvider;
  stop(): void;
  timer: ReturnType<typeof setTimeout> | null;
  /** Until when a refused stream is asked again at once. */
  connectUntil: number;
  /** Reopens since the last face, for the backoff. */
  attempt: number;
  closed: boolean;
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class PocketFaces {
  private session: PocketSession | null = null;
  private shown = false;
  private readonly faces = new Map<string, PocketFace>();
  private readonly streams = new Map<string, Stream>();
  private readonly listeners = new Set<() => void>();
  private unwatch: (() => void) | null = null;
  /** Live faces not written down yet, and when the last write was. */
  private readonly unkept = new Map<string, RendererNode>();
  private keptAt = Number.NEGATIVE_INFINITY;
  private keepTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly deps: PocketFacesDeps) {}

  private now(): number {
    return this.deps.now?.() ?? Date.now();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of this.listeners) listener();
  }

  /** The face to draw for `key` now. */
  face(key: PocketCardKey): PocketFace {
    return this.faces.get(cardId(key)) ?? { tree: null, source: "bundled" };
  }

  /**
   * Follow the signed-in session. The previous session's streams end and its
   * newest faces are written down; the new session's kept faces are read.
   */
  setSession(session: PocketSession | null): void {
    if (
      session?.walletId === this.session?.walletId &&
      session?.networkId === this.session?.networkId
    )
      return;
    this.end();
    this.session = session;
    if (session !== null) {
      const kept = this.readKept(session);
      for (const card of session.cards) {
        const tree = kept[cardId(card)];
        if (tree !== undefined)
          this.faces.set(cardId(card), { tree, source: "kept" });
      }
      if (this.shown) this.start();
    }
    this.changed();
  }

  /** Pocket is on screen or not. The first time it is, the cards' workers start. */
  setShown(shown: boolean): void {
    this.shown = shown;
    if (shown && this.session !== null && this.unwatch === null) this.start();
  }

  /** Send a press or an edit inside `key`'s face to its worker. */
  send(key: PocketCardKey, actionId: string, payload: Uint8Array): void {
    const provider = this.streams.get(cardId(key))?.provider;
    if (provider?.publishRendererAction === undefined) {
      this.deps.log(
        `pocket: ${key.cardId} action ${actionId} not delivered: its worker is not running`,
      );
      return;
    }
    provider
      .publishRendererAction({
        context: context(key),
        actionId,
        payload: bytesToHex(payload) as HexString,
      })
      .catch((error: unknown) =>
        this.deps.log(
          `pocket: ${key.cardId} action ${actionId} not delivered: ${errorText(error)}`,
        ),
      );
  }

  private start(): void {
    const session = this.session;
    if (session === null) return;
    this.unwatch = this.deps.workers.onChange(() => this.follow());
    const byProduct = new Map<string, string[]>();
    for (const card of session.cards)
      byProduct.set(card.productId, [
        ...(byProduct.get(card.productId) ?? []),
        card.cardId,
      ]);
    for (const [productId, cardIds] of byProduct)
      void this.deps.workers.holdForPocket(productId, cardIds);
    this.follow();
  }

  /** Open a stream for each card whose worker serves Pocket, and close the ones whose worker changed or stopped. */
  private follow(): void {
    const session = this.session;
    if (session === null) return;
    for (const card of session.cards) {
      const id = cardId(card);
      const provider = this.deps.workers.serves(card.productId, "pocket")
        ? this.deps.workers.provider(card.productId)
        : null;
      const stream = this.streams.get(id);
      if (stream?.provider === provider) continue;
      if (stream !== undefined) this.close(id);
      if (provider !== null) this.open(card, provider);
    }
  }

  private open(card: PocketCardKey, provider: TrUApiProductProvider): void {
    const id = cardId(card);
    if (provider.render === undefined) {
      this.deps.log(`pocket: ${card.productId}'s worker cannot draw faces`);
      return;
    }
    const render = provider.render.bind(provider);
    const stream: Stream = {
      provider,
      stop: () => {},
      timer: null,
      connectUntil: this.now() + CONNECT_WINDOW_MS,
      attempt: 0,
      closed: false,
    };
    const retry = (why: string) => {
      if (stream.closed) return;
      stream.stop();
      let delay = CONNECT_RETRY_MS;
      if (this.now() >= stream.connectUntil) {
        delay = Math.min(
          REOPEN_MS * 2 ** Math.min(stream.attempt, 5),
          MAX_REOPEN_MS,
        );
        stream.attempt += 1;
        this.deps.log(
          `pocket: ${card.cardId} face stream ended (${why}), reopening in ${delay / 1000}s`,
        );
      }
      stream.timer = setTimeout(connect, delay);
    };
    const connect = () => {
      stream.timer = null;
      if (stream.closed) return;
      stream.stop = render(
        { context: context(card), payload: "0x" as HexString },
        {
          onUpdate: (tree) => {
            if (stream.closed) return;
            stream.attempt = 0;
            stream.connectUntil = 0;
            this.faces.set(id, { tree, source: "live" });
            this.keepSoon(id, tree);
            this.changed();
          },
          onError: (error) => retry(errorText(error)),
          onComplete: () => retry("it completed"),
        },
      );
    };
    this.streams.set(id, stream);
    connect();
  }

  private close(id: string): void {
    const stream = this.streams.get(id);
    if (stream === undefined) return;
    stream.closed = true;
    if (stream.timer !== null) clearTimeout(stream.timer);
    stream.stop();
    this.streams.delete(id);
  }

  /** End the session's streams and write down its newest faces. */
  private end(): void {
    this.unwatch?.();
    this.unwatch = null;
    for (const id of [...this.streams.keys()]) this.close(id);
    this.keepNow();
    this.faces.clear();
    this.keptAt = Number.NEGATIVE_INFINITY;
  }

  private keepSoon(id: string, tree: RendererNode): void {
    this.unkept.set(id, tree);
    const wait = this.keptAt + KEEP_INTERVAL_MS - this.now();
    if (wait <= 0) {
      this.keepNow();
      return;
    }
    this.keepTimer ??= setTimeout(() => this.keepNow(), wait);
  }

  private keepNow(): void {
    if (this.keepTimer !== null) clearTimeout(this.keepTimer);
    this.keepTimer = null;
    const session = this.session;
    if (session === null || this.unkept.size === 0) {
      this.unkept.clear();
      return;
    }
    const kept = this.readKept(session);
    for (const [id, tree] of this.unkept) {
      const json = JSON.stringify(tree, (_key, value: unknown) =>
        typeof value === "bigint" ? Number(value) : value,
      );
      if (json.length > MAX_KEPT_BYTES)
        this.deps.log(`pocket: ${id}'s face is too large to keep`);
      else kept[id] = JSON.parse(json) as RendererNode;
    }
    this.unkept.clear();
    this.keptAt = this.now();
    try {
      this.deps.storage.setItem(
        pocketFacesKey(session.networkId, session.walletId),
        JSON.stringify(kept),
      );
    } catch (error) {
      this.deps.log(`pocket: faces not kept: ${errorText(error)}`);
    }
  }

  private readKept(session: PocketSession): Record<string, RendererNode> {
    try {
      const raw: unknown = JSON.parse(
        this.deps.storage.getItem(
          pocketFacesKey(session.networkId, session.walletId),
        ) ?? "{}",
      );
      if (typeof raw !== "object" || raw === null || Array.isArray(raw))
        return {};
      const kept: Record<string, RendererNode> = {};
      for (const [id, tree] of Object.entries(raw))
        if (
          typeof tree === "object" &&
          tree !== null &&
          typeof (tree as { tag?: unknown }).tag === "string"
        )
          kept[id] = tree as RendererNode;
      return kept;
    } catch {
      return {};
    }
  }
}
