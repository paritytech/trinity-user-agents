import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { CallErrorValue } from "@parity/truapi/scale";
import { err, ok, type Result } from "neverthrow";
import {
  CALL_ID_MAX_BYTES,
  CALL_PRESS_WINDOW_MS,
  CALL_SIGNAL_MAX_BYTES,
  FEATURE,
  INLINE_MEDIA_MAX_BYTES,
  QUERY_TIMEOUTS_MS,
  ROOM_CACHE_LIMIT,
  ROOM_EVENT_QUEUE,
  type BridgePort,
  type ChatFeedUpdate,
  type ChatMessage,
  type ChatQueryRefusal,
  type ChatRoomCall,
  type ChatRoomConfig,
  type ChatRoomEvent,
  type ChatRoomFeatures,
  type ChatRoomMetadata,
  type ChatRoomPreview,
  type ChatV2Envelope,
  type HostChatCapabilitiesResponse,
  type HostChatRoomEventsSubscribeItem,
  type HostQueries,
  type ProductCalls,
  type ProductMethod,
  type QueryMethod,
} from "./contract.js";

/**
 * The host's end of the Chat Extension v2 draft bridge (`./contract.ts`).
 *
 * It keeps what each product declared for its rooms (metadata, preview,
 * configuration, features and call), a page cache of each product-source
 * room, one bridge link per running product worker, and the call media the
 * host runs for a product's call. It answers the product's calls and asks the
 * product the host's queries.
 *
 * Scoping is the host's, not the core's: `ProductWorkers` binds a link to
 * the product id of one `Worker` execution it started, and ends it with the
 * worker and with the session. A product reaches only the rooms it declared
 * through its own link. The core grants nothing here and sees none of it.
 */

/** What the host's call engine reports while it runs one call's media. */
export interface CallMediaEvents {
  /** A signal for the other side's host. The product carries it there. */
  signal(bytes: Uint8Array): void;
  changed(): void;
  /** The media ended on its own, such as when the connection failed. */
  ended(problem: string | null): void;
}

/** The media of one call, as the host's engine runs it. */
export interface CallMediaSession {
  readonly state: "connecting" | "connected";
  readonly local: MediaStream | null;
  readonly remote: MediaStream | null;
  /** A signal of the other side's host. */
  signal(bytes: Uint8Array): void;
  setMuted(muted: boolean): void;
  setCamera(on: boolean): void;
  stop(): void;
}

/** The host's call engine. The product never reaches the devices or the media. */
export interface CallMediaEngine {
  /** What the call screen calls the engine. */
  readonly name: string;
  start(
    call: { callId: string; video: boolean; offerer: boolean },
    events: CallMediaEvents,
  ): CallMediaSession;
}

/** One room a product declared over the bridge. */
export interface V2Room {
  roomId: string;
  metadata: ChatRoomMetadata;
  config: ChatRoomConfig;
  preview?: ChatRoomPreview;
  features?: ChatRoomFeatures;
  call?: ChatRoomCall;
  /** The messages the host has seen, by id. Only a cache: the product keeps them. */
  cache: Map<string, ChatMessage>;
}

/** The media the host runs for one product's call. */
export interface HeldMedia {
  roomId: string;
  callId: string;
  /** The engine's name, such as `synthetic`. */
  engine: string;
  session: CallMediaSession;
}

export type ChatV2Change =
  | { type: "rooms"; productId: string }
  | {
      type: "feed";
      productId: string;
      roomId: string;
      message?: ChatMessage;
      removed?: string;
    }
  | { type: "call"; productId: string }
  /** A link started or ended: what was asked of the old one no longer holds. */
  | { type: "link"; productId: string };

export interface ChatV2HostDeps {
  /** The host's call engine for `productId`'s calls, or null when it runs none for it. */
  media(productId: string): CallMediaEngine | null;
  now?(): number;
  log(text: string): void;
}

export interface QueryOptions {
  timeoutMs?: number;
  signal?: AbortSignal;
}

export type QueryResult<M extends QueryMethod> = Result<
  HostQueries[M][1],
  ChatQueryRefusal<HostQueries[M][2]>
>;

type CallAnswer<M extends ProductMethod> = Result<
  ProductCalls[M][1],
  CallErrorValue<ProductCalls[M][2]>
>;

const MESSAGE_TEXT_MAX = 16 * 1024;
const HELD_SIGNALS_MAX = 32;

const domain = <const E>(value: E): CallErrorValue<E> => ({
  tag: "Domain",
  value,
});
const unknown = (reason: string) =>
  domain({ tag: "Unknown" as const, value: { reason } });
const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);
const utf8Length = (text: string): number =>
  new TextEncoder().encode(text).length;

function bodyTooLarge(message: { body: ChatMessage["body"] }): boolean {
  const body = message.body;
  switch (body.tag) {
    case "Text":
    case "Notice":
      return utf8Length(body.value.text) > MESSAGE_TEXT_MAX;
    case "RichText":
      return utf8Length(body.value.text ?? "") > MESSAGE_TEXT_MAX;
    default:
      return false;
  }
}

/** Apply one reaction change of `participantId` to a message's summaries. */
function react(
  message: ChatMessage,
  participantId: string,
  emoji: string,
  placed: boolean,
): ChatMessage {
  const self = participantId === "user";
  const reactions = message.reactions.map((entry) => ({ ...entry }));
  const entry = reactions.find((summary) => summary.emoji === emoji);
  if (placed) {
    if (entry === undefined)
      reactions.push({ emoji, count: 1, reactedByUser: self });
    else if (!(self && entry.reactedByUser)) {
      entry.count += 1;
      entry.reactedByUser ||= self;
    }
  } else if (entry !== undefined && (!self || entry.reactedByUser)) {
    entry.count -= 1;
    if (self) entry.reactedByUser = false;
  }
  return {
    ...message,
    reactions: reactions.filter((summary) => summary.count > 0),
  };
}

interface PendingQuery {
  resolve(result: Result<unknown, ChatQueryRefusal<unknown>>): void;
}

/** One running product worker's end of the bridge. */
class Link {
  private nextId = 1;
  readonly pending = new Map<number, PendingQuery>();
  subscribed = false;
  closed = false;

  constructor(
    readonly productId: string,
    private readonly port: BridgePort,
  ) {}

  post(message: ChatV2Envelope): void {
    if (!this.closed) this.port.post(message);
  }

  ask(
    method: QueryMethod,
    request: unknown,
    timeoutMs: number,
    signal: AbortSignal | undefined,
  ): Promise<Result<unknown, ChatQueryRefusal<unknown>>> {
    if (signal?.aborted) return Promise.resolve(err({ tag: "Cancelled" }));
    const id = this.nextId++;
    return new Promise((resolve) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const settle = (
        result: Result<unknown, ChatQueryRefusal<unknown>>,
        cancel: boolean,
      ) => {
        if (!this.pending.delete(id)) return;
        if (timer !== undefined) clearTimeout(timer);
        signal?.removeEventListener("abort", onAbort);
        if (cancel) this.post({ kind: "cancel", id });
        resolve(result);
      };
      const onAbort = () => settle(err({ tag: "Cancelled" }), true);
      this.pending.set(id, { resolve: (result) => settle(result, false) });
      if (timeoutMs > 0 && Number.isFinite(timeoutMs))
        timer = setTimeout(
          () => settle(err({ tag: "Timeout" }), true),
          timeoutMs,
        );
      signal?.addEventListener("abort", onAbort, { once: true });
      this.post({
        kind: "query",
        id,
        method,
        request,
      } as ChatV2Envelope);
    });
  }

  close(reason: string): void {
    if (this.closed) return;
    this.closed = true;
    for (const pending of [...this.pending.values()])
      pending.resolve(err({ tag: "HostFailure", value: { reason } }));
    this.pending.clear();
    this.port.close();
  }
}

export class ChatV2Host {
  private readonly declared = new Map<string, Map<string, V2Room>>();
  private readonly links = new Map<string, Link>();
  private readonly epochs = new Map<string, number>();
  private readonly queues = new Map<
    string,
    HostChatRoomEventsSubscribeItem[]
  >();
  private readonly pressed = new Map<string, number>();
  private readonly media = new Map<string, HeldMedia>();
  private readonly heldSignals = new Map<string, Uint8Array[]>();
  private readonly listeners = new Set<(change: ChatV2Change) => void>();

  constructor(private readonly deps: ChatV2HostDeps) {}

  private now(): number {
    return this.deps.now?.() ?? Date.now();
  }

  onChange(listener: (change: ChatV2Change) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(change: ChatV2Change): void {
    for (const listener of [...this.listeners]) listener(change);
  }

  // --- Links ----------------------------------------------------------------

  /** Take the bridge port of `productId`'s worker frame. A new port replaces the last. */
  connect(productId: string, port: BridgePort): void {
    this.disconnect(productId, "The product connected again.");
    const link = new Link(productId, port);
    this.links.set(productId, link);
    this.bump(productId);
    port.listen((message) => {
      if (this.links.get(productId) === link) this.receive(link, message);
    });
  }

  /** End `productId`'s link, as when its worker stops. */
  disconnect(
    productId: string,
    reason = "The product's worker stopped.",
  ): void {
    const link = this.links.get(productId);
    if (link === undefined) return;
    this.links.delete(productId);
    link.close(reason);
    this.stopMedia(productId);
    for (const room of this.roomsOf(productId))
      if (room.call !== undefined) delete room.call;
    this.bump(productId);
    this.emit({ type: "call", productId });
  }

  /** End `productId`'s link and forget what it declared, as when its demo stops. */
  forget(productId: string): void {
    this.disconnect(productId);
    this.queues.delete(productId);
    for (const key of [...this.pressed.keys()])
      if (key.startsWith(`${productId}\n`)) this.pressed.delete(key);
    for (const key of [...this.heldSignals.keys()])
      if (key.startsWith(`${productId}\n`)) this.heldSignals.delete(key);
    if (this.declared.delete(productId))
      this.emit({ type: "rooms", productId });
  }

  /** Forget every link and declaration, as when the session ends. */
  reset(): void {
    for (const productId of [...this.links.keys()]) this.disconnect(productId);
    const products = [...this.declared.keys()];
    this.declared.clear();
    this.queues.clear();
    this.pressed.clear();
    this.heldSignals.clear();
    for (const productId of products) this.emit({ type: "rooms", productId });
  }

  private bump(productId: string): void {
    this.epochs.set(productId, this.epoch(productId) + 1);
    this.emit({ type: "link", productId });
  }

  connected(productId: string): boolean {
    return this.links.has(productId);
  }

  /** Changes whenever `productId`'s link starts or ends. */
  epoch(productId: string): number {
    return this.epochs.get(productId) ?? 0;
  }

  // --- Rooms ----------------------------------------------------------------

  room(productId: string, roomId: string): V2Room | undefined {
    return this.declared.get(productId)?.get(roomId);
  }

  roomsOf(productId: string): V2Room[] {
    return [...(this.declared.get(productId)?.values() ?? [])];
  }

  /** The cached messages of a room, oldest first. */
  cached(productId: string, roomId: string): ChatMessage[] {
    return [...(this.room(productId, roomId)?.cache.values() ?? [])].sort(
      (a, b) => Number(a.timestamp - b.timestamp),
    );
  }

  /** What this host serves end to end over the bridge. */
  capabilities(): HostChatCapabilitiesResponse {
    return {
      features: [
        FEATURE.productSourceRooms,
        FEATURE.participants,
        FEATURE.inlineAttachments,
        FEATURE.callButtons,
      ],
      inlineAttachmentMaxBytes: INLINE_MEDIA_MAX_BYTES,
      preimageAttachmentMaxBytes: 0,
      sealedAttachmentMaxBytes: 0,
    };
  }

  private cache(room: V2Room, message: ChatMessage): void {
    room.cache.delete(message.messageId);
    room.cache.set(message.messageId, message);
    if (room.cache.size <= ROOM_CACHE_LIMIT) return;
    const oldest = [...room.cache.values()].sort((a, b) =>
      Number(a.timestamp - b.timestamp),
    );
    for (const message of oldest.slice(0, room.cache.size - ROOM_CACHE_LIMIT))
      room.cache.delete(message.messageId);
  }

  /** Keep a message the product answered with, and say so to the screen. */
  remember(productId: string, roomId: string, message: ChatMessage): void {
    const room = this.room(productId, roomId);
    if (room !== undefined) this.cache(room, message);
  }

  // --- Host to product ------------------------------------------------------

  /** Ask the product one query. Never rejects. */
  async query<M extends QueryMethod>(
    productId: string,
    method: M,
    request: HostQueries[M][0],
    options: QueryOptions = {},
  ): Promise<QueryResult<M>> {
    const link = this.links.get(productId);
    if (link === undefined)
      return err({
        tag: "HostFailure",
        value: { reason: "The product's worker is not running." },
      });
    const room = this.room(productId, request.roomId);
    if (room === undefined)
      return err(domain({ tag: "UnknownRoom" })) as QueryResult<M>;
    const declared =
      method === "participantsQuery"
        ? room.config.participants
        : room.config.productSource;
    if (!declared)
      return err(
        method === "userAction"
          ? unknown("the host stores this room's messages")
          : domain({ tag: "NotAvailable" }),
      ) as QueryResult<M>;
    const timeoutMs =
      options.timeoutMs ??
      (method === "messagesQuery"
        ? QUERY_TIMEOUTS_MS.messages
        : method === "participantsQuery"
          ? QUERY_TIMEOUTS_MS.details
          : (request as HostQueries["userAction"][0]).action.tag ===
              "NewMessage"
            ? QUERY_TIMEOUTS_MS.newMessage
            : QUERY_TIMEOUTS_MS.userAction);
    const result = (await link.ask(
      method,
      request,
      timeoutMs,
      options.signal,
    )) as QueryResult<M>;
    if (result.isOk())
      this.keepAnswer(productId, method, request, result.value);
    return result;
  }

  private keepAnswer<M extends QueryMethod>(
    productId: string,
    method: M,
    request: HostQueries[M][0],
    item: HostQueries[M][1],
  ): void {
    const room = this.room(productId, request.roomId);
    if (room === undefined) return;
    if (method === "messagesQuery") {
      const page = item as HostQueries["messagesQuery"][1];
      const asked = (request as HostQueries["messagesQuery"][0]).pageSize;
      if (page.messages.length > asked)
        page.messages.splice(0, page.messages.length - asked);
      for (const message of page.messages) this.cache(room, message);
    }
    if (method === "userAction") {
      const answer = item as HostQueries["userAction"][1];
      if (answer.tag === "MessageCreated") this.cache(room, answer.value);
    }
  }

  /**
   * Give the product one room event. It is queued while the product has no
   * `room_events_subscribe` open, the newest {@link ROOM_EVENT_QUEUE} kept.
   * A call press or an accepted call opens the window for `callMedia.start`.
   */
  roomEvent(productId: string, roomId: string, event: ChatRoomEvent): void {
    if (
      event.tag === "CallRequested" ||
      (event.tag === "CallAnswered" && event.value.accept)
    )
      this.pressed.set(`${productId}\n${roomId}`, this.now());
    const item = { roomId, event };
    const link = this.links.get(productId);
    if (link?.subscribed) {
      link.post({ kind: "roomEvent", item });
      return;
    }
    const queue = this.queues.get(productId) ?? [];
    queue.push(item);
    queue.splice(0, Math.max(0, queue.length - ROOM_EVENT_QUEUE));
    this.queues.set(productId, queue);
  }

  /** The media the host runs for `productId`'s call, or null. */
  heldMedia(productId: string): HeldMedia | null {
    return this.media.get(productId) ?? null;
  }

  /** Stop the call media of `productId`, such as when the user hangs up. */
  stopMedia(productId: string): void {
    const held = this.media.get(productId);
    if (held === undefined) return;
    this.media.delete(productId);
    held.session.stop();
    this.emit({ type: "call", productId });
  }

  // --- Product to host ------------------------------------------------------

  private receive(link: Link, message: ChatV2Envelope): void {
    switch (message.kind) {
      case "answer":
        link.pending.get(message.id)?.resolve(ok(message.item));
        return;
      case "refusal":
        link.pending.get(message.id)?.resolve(err(message.error));
        return;
      case "end":
        link.pending.get(message.id)?.resolve(err({ tag: "NoAnswer" }));
        return;
      case "call": {
        let answer: CallAnswer<ProductMethod>;
        try {
          answer = this.call(link, message.method, message.request);
        } catch (error) {
          answer = err({
            tag: "HostFailure",
            value: { reason: errorText(error) },
          });
        }
        link.post(
          answer.isOk()
            ? { kind: "result", id: message.id, ok: true, value: answer.value }
            : {
                kind: "result",
                id: message.id,
                ok: false,
                error: answer.error,
              },
        );
        if (message.method === "roomEventsSubscribe") this.flush(link);
        return;
      }
      default:
        this.deps.log(
          `${link.productId}: chat v2 bridge: ${message.kind} is not for the host`,
        );
    }
  }

  private flush(link: Link): void {
    const queue = this.queues.get(link.productId) ?? [];
    this.queues.delete(link.productId);
    for (const item of queue) link.post({ kind: "roomEvent", item });
  }

  private call(
    link: Link,
    method: ProductMethod,
    request: unknown,
  ): CallAnswer<ProductMethod> {
    const productId = link.productId;
    switch (method) {
      case "createRoom":
        return this.createRoom(
          productId,
          request as ProductCalls["createRoom"][0],
        );
      case "capabilities":
        return ok(this.capabilities());
      case "roomEventsSubscribe":
        link.subscribed = true;
        return ok({});
      case "deleteRoom": {
        const { roomId } = request as ProductCalls["deleteRoom"][0];
        if (!this.declared.get(productId)?.delete(roomId)) return ok({});
        if (this.media.get(productId)?.roomId === roomId)
          this.stopMedia(productId);
        this.emit({ type: "rooms", productId });
        return ok({});
      }
    }
    const { roomId } = request as { roomId: string };
    const room = this.room(productId, roomId);
    if (room === undefined) return err(domain({ tag: "UnknownRoom" }));
    switch (method) {
      case "setRoomMetadata":
        room.metadata = (
          request as ProductCalls["setRoomMetadata"][0]
        ).metadata;
        this.emit({ type: "rooms", productId });
        return ok({});
      case "setRoomPreview":
        room.preview = (request as ProductCalls["setRoomPreview"][0]).preview;
        this.emit({ type: "rooms", productId });
        return ok({});
      case "setRoomFeatures":
        room.features = (
          request as ProductCalls["setRoomFeatures"][0]
        ).features;
        this.emit({ type: "rooms", productId });
        this.emit({ type: "call", productId });
        return ok({});
      case "setRoomCall":
        return this.setRoomCall(
          productId,
          room,
          (request as ProductCalls["setRoomCall"][0]).call,
        );
      case "updateFeed":
        return this.updateFeed(
          productId,
          room,
          (request as ProductCalls["updateFeed"][0]).update,
        );
      case "callMediaStart":
        return this.startMedia(
          productId,
          room,
          request as ProductCalls["callMediaStart"][0],
        );
      case "callMediaSignal": {
        const { callId, signal } =
          request as ProductCalls["callMediaSignal"][0];
        const bytes = hexToBytes(signal);
        if (bytes.length > CALL_SIGNAL_MAX_BYTES)
          return err(
            domain({
              tag: "InvalidRequest",
              value: { reason: "signal is over 64 KiB" },
            }),
          );
        const held = this.media.get(productId);
        if (held?.roomId === roomId && held.callId === callId)
          held.session.signal(bytes);
        else {
          const key = `${productId}\n${roomId}\n${callId}`;
          const signals = this.heldSignals.get(key) ?? [];
          if (signals.length < HELD_SIGNALS_MAX) signals.push(bytes);
          this.heldSignals.set(key, signals);
        }
        return ok({});
      }
      case "callMediaStop": {
        const { callId } = request as ProductCalls["callMediaStop"][0];
        const held = this.media.get(productId);
        if (held?.roomId === roomId && held.callId === callId)
          this.stopMedia(productId);
        this.heldSignals.delete(`${productId}\n${roomId}\n${callId}`);
        return ok({});
      }
    }
  }

  private createRoom(
    productId: string,
    request: ProductCalls["createRoom"][0],
  ): CallAnswer<"createRoom"> {
    let rooms = this.declared.get(productId);
    if (rooms === undefined) {
      rooms = new Map();
      this.declared.set(productId, rooms);
    }
    if (rooms.has(request.roomId)) return ok({ status: "Exists" });
    rooms.set(request.roomId, {
      roomId: request.roomId,
      metadata: request.metadata,
      config: request.config,
      ...(request.preview === undefined ? {} : { preview: request.preview }),
      cache: new Map(),
    });
    this.emit({ type: "rooms", productId });
    return ok({ status: "New" });
  }

  private setRoomCall(
    productId: string,
    room: V2Room,
    call: ChatRoomCall | undefined,
  ): CallAnswer<"setRoomCall"> {
    if (call?.tag === "Active" && call.value.mediaSession !== undefined)
      return err(unknown(`this host does not serve ${FEATURE.roomCalls}`));
    if (call !== undefined && utf8Length(call.value.callId) > CALL_ID_MAX_BYTES)
      return err(unknown("callId is over 128 bytes"));
    const held = this.media.get(productId);
    if (
      held?.roomId === room.roomId &&
      (call?.tag !== "Active" || call.value.callId !== held.callId)
    )
      this.stopMedia(productId);
    if (call === undefined) delete room.call;
    else room.call = call;
    this.emit({ type: "call", productId });
    return ok({});
  }

  private updateFeed(
    productId: string,
    room: V2Room,
    update: ChatFeedUpdate,
  ): CallAnswer<"updateFeed"> {
    if (!room.config.productSource)
      return err(
        unknown(
          "this draft host keeps no v2 feed for a host-store room; post with chat.postMessage",
        ),
      );
    const roomId = room.roomId;
    const changed = (message: ChatMessage) => {
      this.cache(room, message);
      this.emit({ type: "feed", productId, roomId, message });
      this.emit({ type: "rooms", productId });
      return ok({});
    };
    switch (update.tag) {
      case "New":
        if (room.cache.has(update.value.messageId))
          return err(domain({ tag: "DuplicateMessage" }));
        if (bodyTooLarge(update.value))
          return err(domain({ tag: "MessageTooLarge" }));
        return changed(update.value);
      case "Deleted":
        room.cache.delete(update.value.messageId);
        this.emit({
          type: "feed",
          productId,
          roomId,
          removed: update.value.messageId,
        });
        this.emit({ type: "rooms", productId });
        return ok({});
    }
    const known = room.cache.get(update.value.messageId);
    // A product-source room keeps only a cache: a change to a message the
    // host has not seen applies when the product pages it in.
    if (known === undefined) return ok({});
    switch (update.tag) {
      case "Edited":
        if (bodyTooLarge(update.value))
          return err(domain({ tag: "MessageTooLarge" }));
        return changed({
          ...known,
          body: update.value.body,
          editedAt: update.value.editedAt,
        });
      case "StatusChanged":
        return changed({ ...known, status: update.value.status });
      case "ReactionPlaced":
      case "ReactionRemoved":
        return changed(
          react(
            known,
            update.value.participantId,
            update.value.emoji,
            update.tag === "ReactionPlaced",
          ),
        );
    }
  }

  private startMedia(
    productId: string,
    room: V2Room,
    request: ProductCalls["callMediaStart"][0],
  ): CallAnswer<"callMediaStart"> {
    const engine = this.deps.media(productId);
    if (engine === null) return err({ tag: "Unsupported" });
    const call = room.call;
    if (
      call?.tag !== "Active" ||
      call.value.callId !== request.callId ||
      call.value.video !== request.video
    )
      return err(
        domain({
          tag: "InvalidRequest",
          value: { reason: "the room shows no such active call" },
        }),
      );
    const pressedAt = this.pressed.get(`${productId}\n${room.roomId}`);
    if (
      pressedAt === undefined ||
      this.now() - pressedAt > CALL_PRESS_WINDOW_MS
    )
      return err(domain({ tag: "NotAllowed" }));
    const current = this.media.get(productId);
    if (current?.roomId === room.roomId && current.callId === request.callId)
      return ok({});
    this.stopMedia(productId);
    const roomId = room.roomId;
    const callId = request.callId;
    let held: HeldMedia | undefined;
    const session = engine.start(
      { callId, video: request.video, offerer: request.offerer },
      {
        signal: (bytes) =>
          this.roomEvent(productId, roomId, {
            tag: "CallSignal",
            value: { callId, signal: bytesToHex(bytes) },
          }),
        changed: () => this.emit({ type: "call", productId }),
        ended: (problem) => {
          if (held === undefined || this.media.get(productId) !== held) return;
          this.media.delete(productId);
          if (problem !== null)
            this.deps.log(`${productId}: call media ended: ${problem}`);
          this.emit({ type: "call", productId });
        },
      },
    );
    held = { roomId, callId, engine: engine.name, session };
    this.media.set(productId, held);
    const key = `${productId}\n${roomId}\n${callId}`;
    for (const bytes of this.heldSignals.get(key) ?? []) session.signal(bytes);
    this.heldSignals.delete(key);
    this.emit({ type: "call", productId });
    return ok({});
  }
}
