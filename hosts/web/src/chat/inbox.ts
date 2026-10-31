import type { ChatContact, ChatProvider, ProviderEvent } from "./provider.js";
import type { ChatV2Host } from "./draft/host.js";
import {
  ChatV2Provider,
  type ChatV2ProviderOptions,
} from "./draft/provider.js";
import {
  ProductRoomProvider,
  type ProductRoomDeps,
} from "./product-room-provider.js";

/**
 * The chat providers of the signed-in session, as the Chat screen sees them.
 *
 * One session (a wallet or a test user, on one network) has one provider per
 * product whose worker is listed or has rooms, and, when the host runs the
 * Chat Extension v2 draft bridge, one chat product provider per listed
 * worker. All of them run for the session, whichever screen or conversation
 * is in front, and all of them are disposed when the session ends or
 * changes, so nothing one session started answers into the next.
 */

export type InboxChange =
  | { type: "session" }
  | { type: "providers" }
  | { type: "provider"; providerId: string; event: ProviderEvent };

/** What the Chat screen reads from the inbox. */
export interface ChatInboxView {
  signedIn(): boolean;
  providers(): readonly ChatProvider[];
  onChange(listener: (change: InboxChange) => void): () => void;
}

export interface ChatInboxDeps {
  rooms: ProductRoomDeps;
  /** The products whose workers are listed for the session. */
  workerNames(): string[];
  log(text: string): void;
  /**
   * The host end of the Chat Extension v2 draft bridge. Each listed worker
   * then also gets a provider for the rooms it declares there.
   */
  chatV2?: ChatV2Host;
  /** What the host lends `productId`'s v2 provider: names for its contact rooms, and starting chats. */
  chatV2Options?(productId: string): ChatV2ProviderOptions;
}

/** A signed-in session: a wallet's, or a test user's, which has no wallet keys. */
export interface InboxSession {
  walletId: string;
  network: string;
  /** Tells apart two sessions of one wallet, such as another identity. */
  identity?: string;
}

const sessionKey = (session: InboxSession): string =>
  `${session.network}:${session.walletId}:${session.identity ?? ""}`;

export class ChatInbox implements ChatInboxView {
  private session: InboxSession | null = null;
  private key: string | null = null;
  private readonly products = new Map<string, ProductRoomProvider>();
  private readonly v2Products = new Map<string, ChatV2Provider>();
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

  providers(): ChatProvider[] {
    return [...this.products.values(), ...this.v2Products.values()];
  }

  /**
   * The people of the chat products' contact rooms, named by the host, for
   * the host's own New chat picker only. They are not contacts: no other
   * product gets them, and none is a payment recipient.
   */
  roomPeople(): ChatContact[] {
    return [...this.v2Products.values()].flatMap((provider) =>
      provider.people(),
    );
  }

  /** Follow the signed-in session. A different wallet, network or identity ends every provider. */
  setSession(session: InboxSession | null): void {
    const key = session === null ? null : sessionKey(session);
    this.session = session;
    if (key === this.key) return;
    this.key = key;
    for (const provider of this.providers()) this.remove(provider);
    this.products.clear();
    this.v2Products.clear();
    if (session !== null) this.syncProducts(false);
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
    if (this.syncV2Products()) changed = true;
    if (changed && announce) this.emit({ type: "providers" });
  }

  /** One v2 draft provider per listed worker, when the host runs the bridge. */
  private syncV2Products(): boolean {
    const host = this.deps.chatV2;
    if (host === undefined) return false;
    const wanted = new Set(
      this.session === null ? [] : this.deps.workerNames(),
    );
    let changed = false;
    for (const [productId, provider] of this.v2Products)
      if (!wanted.has(productId)) {
        this.remove(provider);
        this.v2Products.delete(productId);
        changed = true;
      }
    for (const productId of wanted)
      if (!this.v2Products.has(productId)) {
        this.v2Products.set(
          productId,
          this.add(
            new ChatV2Provider(
              productId,
              host,
              Date.now,
              this.deps.chatV2Options?.(productId) ?? {},
            ),
          ),
        );
        changed = true;
      }
    return changed;
  }
}
