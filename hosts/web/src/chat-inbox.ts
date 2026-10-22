import type { ChatDiagnostics } from "./chat/diagnostics.js";
import type { ChatProvider, ProviderEvent } from "./chat-provider.js";
import {
  DraftChatProvider,
  draftCacheKey,
  draftEnabledKey,
  type DraftPort,
} from "./draft-chat.js";
import { startNovaChat, type ChatSession } from "./nova-chat-provider.js";
import {
  ProductRoomProvider,
  type ProductRoomDeps,
} from "./product-room-provider.js";

/**
 * The chat providers of the signed-in session, as the Chat screen sees them.
 *
 * One session (wallet, network and identity) has: the host's Nova chat, one
 * provider per product whose worker is listed or has rooms, and the draft
 * worker-owned chat when the user turned it on. All of them run for the
 * session, whichever screen or conversation is in front, and all of them are
 * disposed when the session ends or changes, so nothing one session started
 * answers into the next.
 */

export type InboxChange =
  | { type: "session" }
  | { type: "providers" }
  | { type: "provider"; providerId: string; event: ProviderEvent };

/** What the Chat screen reads from the inbox. */
export interface ChatInboxView {
  signedIn(): boolean;
  /** Why the host's own chat is off for this session, or null. */
  problem(): string | null;
  providers(): readonly ChatProvider[];
  onChange(listener: (change: InboxChange) => void): () => void;
}

export interface ChatInboxDeps {
  storage: Storage;
  diagnostics: ChatDiagnostics;
  rooms: ProductRoomDeps;
  /** The products whose workers are listed for the session. */
  workerNames(): string[];
  /** Start the draft fixture's worker. */
  draftWorker(): DraftPort;
  log(text: string): void;
}

export const DRAFT_PROVIDER_ID = "draft";

const sessionKey = (session: ChatSession): string =>
  `${session.network}:${session.walletId}:${session.identityAccountId ?? ""}:${session.chatPublicKey ?? ""}`;

export class ChatInbox implements ChatInboxView {
  private session: ChatSession | null = null;
  private key: string | null = null;
  private refusal: string | null = null;
  private nova: ChatProvider | null = null;
  private draft: DraftChatProvider | null = null;
  private readonly products = new Map<string, ProductRoomProvider>();
  private readonly unsubscribe = new Map<string, () => void>();
  private readonly listeners = new Set<(change: InboxChange) => void>();

  constructor(private readonly deps: ChatInboxDeps) {
    deps.rooms.chat.onChange(() => this.syncProducts());
    deps.rooms.onWorkersChange(() => this.syncProducts());
  }

  onChange(listener: (change: InboxChange) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(change: InboxChange): void {
    for (const listener of [...this.listeners]) listener(change);
  }

  signedIn(): boolean {
    return this.session !== null;
  }

  problem(): string | null {
    return this.refusal;
  }

  providers(): ChatProvider[] {
    return [
      ...(this.nova === null ? [] : [this.nova]),
      ...this.products.values(),
      ...(this.draft === null ? [] : [this.draft]),
    ];
  }

  /** Follow the signed-in session. A different wallet, network or identity ends every provider. */
  setSession(session: ChatSession | null): void {
    const key = session === null ? null : sessionKey(session);
    this.session = session;
    if (key === this.key) return;
    this.key = key;
    for (const provider of this.providers()) this.remove(provider);
    this.nova = null;
    this.draft = null;
    this.products.clear();
    // Events of one session are not shown in the next.
    this.deps.diagnostics.clear();
    this.refusal = null;
    if (session !== null) {
      const nova = startNovaChat(session, this.deps);
      if (typeof nova === "string") this.refusal = nova;
      else this.nova = this.add(nova);
      if (this.draftEnabled()) this.draft = this.add(this.newDraft(session));
      this.syncProducts(false);
    }
    this.emit({ type: "session" });
  }

  private add<T extends ChatProvider>(provider: T): T {
    this.unsubscribe.set(
      provider.id,
      provider.subscribe((event) =>
        this.emit({ type: "provider", providerId: provider.id, event }),
      ),
    );
    return provider;
  }

  private remove(provider: ChatProvider): void {
    this.unsubscribe.get(provider.id)?.();
    this.unsubscribe.delete(provider.id);
    provider.dispose();
  }

  /** One provider per product with a listed worker or a room. */
  private syncProducts(announce = true): void {
    const wanted = new Set(
      this.session === null
        ? []
        : [
            ...this.deps.workerNames(),
            ...this.deps.rooms.chat.rooms().map((room) => room.productId),
          ],
    );
    let changed = false;
    for (const [productId, provider] of this.products)
      if (!wanted.has(productId)) {
        this.remove(provider);
        this.products.delete(productId);
        changed = true;
      }
    for (const productId of wanted)
      if (!this.products.has(productId)) {
        this.products.set(
          productId,
          this.add(new ProductRoomProvider(productId, this.deps.rooms)),
        );
        changed = true;
      }
    if (changed && announce) this.emit({ type: "providers" });
  }

  private newDraft(session: ChatSession): DraftChatProvider {
    return new DraftChatProvider({
      id: DRAFT_PROVIDER_ID,
      label: "Draft worker chat",
      connect: this.deps.draftWorker,
      storage: this.deps.storage,
      cacheKey: draftCacheKey(session.network, session.walletId),
      log: this.deps.log,
    });
  }

  /** Whether the user turned the draft worker-owned chat on for this wallet and network. */
  draftEnabled(): boolean {
    const session = this.session;
    return (
      session !== null &&
      this.deps.storage.getItem(
        draftEnabledKey(session.network, session.walletId),
      ) === "on"
    );
  }

  /** Turn the draft worker-owned chat on or off for this wallet and network. */
  setDraftEnabled(on: boolean): void {
    const session = this.session;
    if (session === null || on === this.draftEnabled()) return;
    const key = draftEnabledKey(session.network, session.walletId);
    if (on) {
      this.deps.storage.setItem(key, "on");
      this.draft = this.add(this.newDraft(session));
    } else {
      this.deps.storage.removeItem(key);
      if (this.draft !== null) this.remove(this.draft);
      this.draft = null;
    }
    this.emit({ type: "providers" });
  }

  /** Stop the draft worker and start a fresh one, as a crash or an update would. */
  restartDraft(): void {
    this.draft?.restart();
  }
}
