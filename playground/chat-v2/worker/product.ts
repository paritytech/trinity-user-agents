/**
 * The sample product: start-up, the six query handlers, the renderer, and
 * the events of `action_subscribe`. `index.ts` starts it with the client of
 * the Worker; a test starts it with the client of a test host.
 */
import type {
  CallErrorValue,
  ChatEvent,
  ChatMessageBody,
  ChatUserAction,
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductChatQueryError,
  ProductChatUserActionError,
  ProductChatUserActionItem,
  ProductRendererRenderRequest,
  RendererNode,
  VersionedProductChatQueryError,
  VersionedProductChatUserActionError,
  VersionedProductRendererRenderError,
} from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import { hexToUtf8, utf8ToHex } from "./bytes";
import { Clock, type RoomId } from "./clock";
import { parseCommand, parseCommandEvent, type ParsedCommand } from "./commands";
import type { ProductContext, SampleClient } from "./context";
import { call, describeCallError } from "./errors";
import { helpCommand, runCommand, setActivity, setPreview } from "./execute";
import { Feed } from "./feed";
import {
  CARD_MESSAGE_TYPE,
  MISSING_PREIMAGE_KEY,
  pickedBody,
  productSourceConfig,
  hostStoreConfig,
  roomMetadata,
  sourcePreview,
  type Assets,
} from "./fixtures";
import { PRODUCT } from "./links";
import { EventLog, type LogSink, type QueryName } from "./log";
import {
  answerMessageActions,
  answerMessages,
  answerParticipantActions,
  answerParticipants,
  answerReactions,
  knownRoom,
  type Answer,
  type QueryContext,
} from "./queries";
import { CARD_LIKE_ACTION, cardTree, FOOTER_PING_ACTION, footerTree } from "./render";
import {
  decodeState,
  encodeState,
  initialState,
  memoryStorage,
  type ProductState,
  type ProductStorage,
  type RoomSettings,
} from "./state";
import { SourceStore } from "./store";

export const STATE_KEY = "chat-v2/state";
export const BOOT_KEY = "chat-v2/boot";

export interface SampleProductOptions {
  client: SampleClient;
  assets: Assets;
  /** The product local storage; the default uses `client.localStorage`. */
  storage?: ProductStorage;
  /** Where the event log goes; the default is `console.info`. */
  logSink?: LogSink;
  /** The DotNS name of the product, for the links. */
  product?: string;
  /** What `/fail crash` does; the default throws outside any promise. */
  crash?: () => void;
}

/** The storage of the product, over `localStorage` of the host. */
export function hostStorage(client: SampleClient): ProductStorage {
  return {
    async read(key) {
      const result = await client.localStorage.read({ key });
      if (result.isErr()) throw new Error(`local_storage read ${describeCallError(result.error)}`);
      return result.value.value === undefined ? undefined : hexToUtf8(result.value.value);
    },
    async write(key, value) {
      const result = await client.localStorage.write({ key, value: utf8ToHex(value) });
      if (result.isErr()) throw new Error(`local_storage write ${describeCallError(result.error)}`);
    },
  };
}

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function queryError(error: ProductChatQueryError): CallErrorValue<VersionedProductChatQueryError> {
  return { tag: "Domain", value: { tag: "V1", value: error } };
}

function userActionError(error: ProductChatUserActionError): CallErrorValue<VersionedProductChatUserActionError> {
  return { tag: "Domain", value: { tag: "V1", value: error } };
}

/** One line for a body, for the echo messages. */
export function summarizeBody(body: ChatMessageBody): string {
  switch (body.tag) {
    case "Text":
      return body.value.text;
    case "RichText":
      return [body.value.text ?? "", body.value.media.map((item) => `[${item.tag}]`).join(" ")].filter(Boolean).join(" ");
    case "Actions":
      return `[Actions] ${body.value.text ?? ""}`.trim();
    case "Custom":
      return `[Custom ${body.value.messageType}]`;
  }
}

function bodyText(body: ChatMessageBody): string | undefined {
  if (body.tag === "Text") return body.value.text;
  if (body.tag === "RichText") return body.value.text;
  return undefined;
}

/** The summary of an echo: the event and what it names. */
export function echoSummary(event: ChatEvent | ChatUserAction): string {
  switch (event.tag) {
    case "MessagePosted":
    case "NewMessage":
      return summarizeBody(event.value.body);
    case "UserEdited":
    case "EditMessage":
      return `${event.value.messageId} ${summarizeBody(event.value.body)}`;
    case "UserReacted":
      return `${event.value.messageId} ${event.value.emoji} ${event.value.placed ? "placed" : "removed"}`;
    case "ReactionPlaced":
    case "ReactionRemoved":
      return `${event.value.messageId} ${event.value.emoji}`;
    case "UserDeleted":
    case "DeleteMessage":
    case "Seen":
    case "Retry":
      return event.value.messageId;
    default:
      return "";
  }
}

export class SampleProduct {
  /** Resolves once the state is loaded and the log has its `boot`. */
  readonly ready: Promise<void>;
  /** Resolves once the rooms of the start exist. */
  readonly rooms: Promise<void>;

  readonly #client: SampleClient;
  readonly #assets: Assets;
  readonly #storage: ProductStorage;
  readonly #product: string;
  readonly #crash: () => void;
  readonly #logSink: LogSink | undefined;
  #log: EventLog | undefined;
  #state: ProductState = initialState();
  #store = new SourceStore(this.#state.source);
  #clock = new Clock(this.#state.clock);
  #preimage: Promise<HexString> = Promise.resolve(MISSING_PREIMAGE_KEY);
  #queue: Promise<void> = Promise.resolve();
  #saves: Promise<void> = Promise.resolve();
  /** The storage in use: the host's, or memory when the host has none. */
  #storageInUse: ProductStorage | undefined;
  readonly #feed: Feed;

  constructor(options: SampleProductOptions) {
    this.#client = options.client;
    this.#assets = options.assets;
    this.#storage = options.storage ?? hostStorage(options.client);
    this.#product = options.product ?? PRODUCT;
    this.#logSink = options.logSink;
    this.#crash =
      options.crash ??
      (() =>
        setTimeout(() => {
          throw new Error("/fail crash: the sample product stops its worker");
        }, 0));
    this.#feed = new Feed({
      chat: this.#client.chat,
      clock: () => this.#clock,
      store: () => this.#store,
      settings: (room) => this.#settings(room),
    });

    // 0. The state load. The client gives a query it kept to the handler
    //    during the registration, and that handler waits for `ready`, so
    //    `ready` exists before the first handler does.
    this.ready = this.#load();
    // 1. The handlers, before the first `await`, so no query is lost.
    this.#registerQueries();
    this.#client.renderer.onRender((request, send, interrupt) => this.#render(request, send, interrupt));
    // 2. The subscriptions.
    this.#client.chat.actionSubscribe().subscribe({
      next: (item) => this.#received(item),
      error: (error) => console.error("chat-v2: action_subscribe ended", error),
    });
    this.#client.renderer.actionSubscribe().subscribe({
      next: (item) => this.#rendererAction(item),
      error: (error) => console.error("chat-v2: renderer action_subscribe ended", error),
    });
    // 3. The rooms. A room that exists keeps what it declared.
    this.rooms = this.#createRooms();
  }

  /** Every queued task has run, and the state is saved. */
  async idle(): Promise<void> {
    await this.ready;
    let queue: Promise<void>;
    do {
      queue = this.#queue;
      await queue;
    } while (queue !== this.#queue);
    await this.#saves;
  }

  get state(): ProductState {
    return this.#state;
  }

  get store(): SourceStore {
    return this.#store;
  }

  // ---------------------------------------------------------------- start

  async #createRooms(): Promise<void> {
    const chat = this.#client.chat;
    const results = await Promise.all([
      chat.createRoom({ roomId: "host", metadata: roomMetadata("host", this.#assets), config: hostStoreConfig() }),
      chat.createRoom({
        roomId: "source",
        metadata: roomMetadata("source", this.#assets),
        config: productSourceConfig(),
        preview: sourcePreview(),
      }),
    ]);
    for (const result of results) {
      if (result.isErr()) console.error(`chat-v2: create_room failed: ${describeCallError(result.error)}`);
    }
  }

  async #load(): Promise<void> {
    let storage = this.#storage;
    let boot = 1;
    try {
      boot = Number((await storage.read(BOOT_KEY)) ?? "0") + 1;
      await storage.write(BOOT_KEY, String(boot));
    } catch (error) {
      console.error("chat-v2: local storage is not available; the state lives in memory", error);
      storage = memoryStorage();
    }
    this.#storageInUse = storage;
    try {
      const saved = await storage.read(STATE_KEY);
      if (saved !== undefined) this.#state = decodeState(saved);
    } catch (error) {
      console.error("chat-v2: the saved state does not load; the product starts again", error);
      this.#state = initialState();
    }
    this.#store = new SourceStore(this.#state.source);
    this.#clock = new Clock(this.#state.clock);
    this.#log = new EventLog(boot, this.#logSink);
    this.#preimage = this.#preparePreimage();
  }

  /**
   * Submit `assets/sample.png` at the first start and keep the key. A host
   * that refuses leaves the key of no preimage, so `/post rich` then shows
   * the missing-preimage placeholder.
   */
  async #preparePreimage(): Promise<HexString> {
    let submitted = this.#state.preimageKey !== undefined;
    if (!submitted) {
      const result = await this.#client.preimage.submit(this.#assets.samplePng);
      submitted = result.isOk();
      this.#state.preimageKey = result.isOk() ? result.value : MISSING_PREIMAGE_KEY;
      if (result.isErr()) console.error(`chat-v2: preimage submit failed: ${describeCallError(result.error)}`);
      this.#save();
    }
    const key = this.#state.preimageKey ?? MISSING_PREIMAGE_KEY;
    this.#record({ via: "preimage", room: null, kind: key === MISSING_PREIMAGE_KEY ? "Missing" : "Submitted", data: { key } });
    return key;
  }

  // ------------------------------------------------------------- plumbing

  #settings(room: RoomId): RoomSettings {
    return this.#state.rooms[room];
  }

  #context(): ProductContext {
    return {
      chat: this.#client.chat,
      assets: this.#assets,
      product: this.#product,
      state: this.#state,
      clock: this.#clock,
      store: this.#store,
      feed: this.#feed,
      settings: (room) => this.#settings(room),
      preimageKey: () => this.#preimage,
    };
  }

  #queryContext(): QueryContext {
    return { rooms: this.#state.rooms, store: this.#store, product: this.#product };
  }

  #record(entry: Parameters<EventLog["record"]>[0]): number {
    if (!this.#log) throw new Error("the event log is not ready");
    return this.#log.record(entry);
  }

  /** Run tasks one at a time, in the order they arrive, then save. */
  #enqueue(task: () => Promise<void>): Promise<void> {
    const run = this.#queue.then(task).catch((error: unknown) => {
      console.error("chat-v2: a task failed", error);
    });
    this.#queue = run.then(() => this.#save());
    return run;
  }

  #save(): Promise<void> {
    this.#saves = this.#saves.then(async () => {
      this.#state.source = this.#store.snapshot();
      try {
        await (this.#storageInUse ?? this.#storage).write(STATE_KEY, encodeState(this.#state));
      } catch (error) {
        console.error("chat-v2: the state does not save", error);
      }
    });
    return this.#saves;
  }

  async #run(room: RoomId, command: ParsedCommand | undefined): Promise<void> {
    if (command) await runCommand(this.#context(), room, command);
  }

  async #echo(room: RoomId, event: ChatEvent | ChatUserAction): Promise<void> {
    const settings = this.#settings(room);
    if (!settings.echo) return;
    // A `Seen` of an echo makes no new echo: the new echo is the newest
    // message, so the host reports it as seen, and the two loop forever.
    if (event.tag === "Seen" && settings.echoMessages.includes(event.value.messageId)) return;
    const summary = echoSummary(event);
    await this.#feed.reply(room, `echo ${event.tag}${summary === "" ? "" : ` ${summary}`}`, { echo: true });
  }

  // -------------------------------------------------------------- queries

  #registerQueries(): void {
    const chat = this.#client.chat;
    chat.onMessagesQuery((request, send, interrupt) =>
      this.#query("messages_query", request.roomId, request.anchor.tag, request, send, interrupt, () =>
        answerMessages(this.#queryContext(), request),
      () => "Page"),
    );
    chat.onReactionsQuery((request, send, interrupt) =>
      this.#query("reactions_query", request.roomId, "Reactions", request, send, interrupt, () =>
        answerReactions(this.#queryContext(), request),
      () => "Reactions"),
    );
    chat.onParticipantsQuery((request, send, interrupt) =>
      this.#query("participants_query", request.roomId, request.query.tag, request, send, interrupt, () =>
        answerParticipants(this.#queryContext(), request),
      (item) => item.tag),
    );
    chat.onMessageActionsQuery((request, send, interrupt) =>
      this.#query("message_actions_query", request.roomId, "MessageActions", request, send, interrupt, () =>
        answerMessageActions(this.#queryContext(), request),
      (item) => item.menu.tag),
    );
    chat.onParticipantActionsQuery((request, send, interrupt) =>
      this.#query("participant_actions_query", request.roomId, "ParticipantActions", request, send, interrupt, () =>
        answerParticipantActions(this.#queryContext(), request),
      (item) => item.menu.tag),
    );
    chat.onUserAction((request, send, interrupt) => this.#userAction(request, send, interrupt));
  }

  /**
   * Serve one query: log it, wait for `/slow`, apply `/fail unavailable`,
   * answer with one item or an interrupt, and log the answer. A host that
   * stops the query before the answer gets a `Timeout` line.
   */
  #query<Request, Item>(
    via: QueryName,
    roomId: string,
    kind: string,
    request: Request,
    send: (item: Item) => void,
    interrupt: (reason?: CallErrorValue<VersionedProductChatQueryError>) => void,
    answer: () => Answer<Item>,
    answerKind: (item: Item) => string,
  ): () => void {
    let seq = 0;
    let done = false;
    const serve = async () => {
      await this.ready;
      seq = this.#record({ via, room: roomId, kind, data: request });
      if (this.#state.slowMs > 0) await delay(this.#state.slowMs);
      if (done) return;
      let result: Answer<Item>;
      if (this.#state.fail.unavailable) {
        this.#state.fail.unavailable = false;
        void this.#save();
        result = { error: { tag: "NotAvailable" } };
      } else {
        result = answer();
      }
      done = true;
      if ("ok" in result) {
        send(result.ok);
        this.#record({ via: "answer", re: seq, room: roomId, kind: answerKind(result.ok), data: result.ok });
      } else {
        interrupt(queryError(result.error));
        this.#record({ via: "answer", re: seq, room: roomId, kind: result.error.tag, data: result.error });
      }
    };
    serve().catch((error: unknown) => {
      if (done) return;
      done = true;
      const reason = error instanceof Error ? error.message : String(error);
      interrupt(queryError({ tag: "Unknown", value: { reason } }));
      this.#record({ via: "answer", re: seq, room: roomId, kind: "Unknown", data: { reason } });
    });
    return () => this.#stopped(() => done, () => (done = true), () => seq, roomId);
  }

  /** The host stopped a query: log `Timeout` if the product did not answer. */
  #stopped(isDone: () => boolean, markDone: () => void, seq: () => number, roomId: string): void {
    if (isDone()) return;
    markDone();
    void this.ready.then(() => this.#record({ via: "answer", re: seq(), room: roomId, kind: "Timeout", data: null }));
  }

  #userAction(
    request: { roomId: string; action: ChatUserAction },
    send: (item: ProductChatUserActionItem) => void,
    interrupt: (reason?: CallErrorValue<VersionedProductChatUserActionError>) => void,
  ): () => void {
    let seq = 0;
    let done = false;
    const answer = (outcome: { ok: ProductChatUserActionItem } | { error: ProductChatUserActionError }) => {
      if (done) return;
      done = true;
      if ("ok" in outcome) {
        send(outcome.ok);
        this.#record({ via: "answer", re: seq, room: request.roomId, kind: outcome.ok.tag, data: outcome.ok.value ?? null });
      } else {
        interrupt(userActionError(outcome.error));
        this.#record({ via: "answer", re: seq, room: request.roomId, kind: outcome.error.tag, data: outcome.error.value ?? null });
      }
    };
    const serve = async () => {
      await this.ready;
      const { action } = request;
      seq = this.#record({ via: "user_action", room: request.roomId, kind: action.tag, data: action.value });
      if (this.#state.slowMs > 0) await delay(this.#state.slowMs);
      if (done) return;
      if (request.roomId !== "source") return answer({ error: { tag: "UnknownRoom" } });
      const fail = this.#state.fail;
      if (fail.crash) {
        fail.crash = false;
        await this.#save();
        this.#crash();
        return;
      }
      if (fail.timeout) {
        // No answer: the host waits, then marks the message `Failed`.
        fail.timeout = false;
        void this.#save();
        return;
      }
      if (fail.reject) {
        fail.reject = false;
        void this.#save();
        return answer({ error: { tag: "Rejected", value: { reason: "Rejected by /fail" } } });
      }
      await this.#enqueue(async () => {
        if (done) return;
        const followUp = await this.#applyUserAction(action, answer);
        await followUp?.();
      });
    };
    serve().catch((error: unknown) => {
      answer({ error: { tag: "Unknown", value: { reason: error instanceof Error ? error.message : String(error) } } });
    });
    return () => this.#stopped(() => done, () => (done = true), () => seq, request.roomId);
  }

  /**
   * Apply a user action of `source` to the store and answer it. The result
   * is the work that follows the answer: the echo and a command.
   */
  async #applyUserAction(
    action: ChatUserAction,
    answer: (outcome: { ok: ProductChatUserActionItem } | { error: ProductChatUserActionError }) => void,
  ): Promise<(() => Promise<void>) | undefined> {
    const room: RoomId = "source";
    const store = this.#store;
    const echo = () => this.#echo(room, action);
    if (action.tag === "NewMessage") {
      const message = {
        messageId: this.#clock.nextUserId(),
        origin: { participantId: "user" },
        timestamp: this.#clock.nextTimestamp(),
        body: action.value.body,
        status: "Sent" as const,
        ...(action.value.replyTo === undefined ? {} : { replyTo: action.value.replyTo }),
        reactions: [],
      };
      store.add(message);
      answer({ ok: { tag: "MessageCreated", value: message } });
      const text = bodyText(action.value.body);
      return async () => {
        await echo();
        await this.#run(room, text === undefined ? undefined : parseCommand(text));
      };
    }
    if (action.tag === "Retry") {
      const { messageId } = action.value;
      try {
        await call(
          "update_feed",
          this.#client.chat.updateFeed({ roomId: room, update: { tag: "StatusChanged", value: { messageId, status: "Sent" } } }),
        );
        if (store.has(messageId)) store.setStatus(messageId, "Sent");
      } catch (error) {
        console.error("chat-v2: retry status update failed", error);
      }
      answer({ ok: { tag: "Acknowledged" } });
      return echo;
    }
    if (action.tag === "Seen") {
      try {
        await setPreview(this.#context(), room, { badge: { tag: "UnreadCount", value: { count: 0 } } });
      } catch (error) {
        console.error("chat-v2: seen preview update failed", error);
      }
      answer({ ok: { tag: "Acknowledged" } });
      return echo;
    }
    const { messageId } = action.value;
    if (!store.has(messageId)) {
      answer({ error: { tag: "UnknownMessage" } });
      return undefined;
    }
    switch (action.tag) {
      case "EditMessage":
        store.edit(messageId, action.value.body, this.#clock.now());
        break;
      case "DeleteMessage":
        store.delete(messageId);
        break;
      case "ReactionPlaced":
      case "ReactionRemoved":
        store.react(messageId, "user", action.value.emoji, action.tag === "ReactionPlaced");
        break;
    }
    answer({ ok: { tag: "Acknowledged" } });
    return echo;
  }

  // --------------------------------------------------------------- events

  #received(item: HostChatActionSubscribeItem): void {
    void this.ready.then(() => {
      this.#record({ via: "action_subscribe", room: item.roomId, kind: item.event.tag, data: item.event.value ?? null });
      const room = knownRoom(item.roomId);
      if (room) void this.#enqueue(() => this.#handleEvent(room, item.event));
    });
  }

  async #handleEvent(room: RoomId, event: ChatEvent): Promise<void> {
    switch (event.tag) {
      case "MessagePosted": {
        await this.#echo(room, event);
        const text = bodyText(event.value.body);
        return this.#run(room, text === undefined ? undefined : parseCommand(text));
      }
      case "Command":
        return this.#run(room, parseCommandEvent(event.value));
      case "ActionTriggered": {
        const body = pickedBody(event.value.actionId);
        if (!body) return;
        try {
          await this.#feed.edit(room, event.value.messageId, body);
        } catch (error) {
          await this.#feed.reply(room, `error ActionTriggered: ${describeCallError(error)}`);
        }
        return;
      }
      case "UserReacted":
        this.#feed.recordReaction(room, event.value.messageId, "user", event.value.emoji, event.value.placed);
        return this.#echo(room, event);
      case "UserEdited":
      case "UserDeleted":
      case "Seen":
      case "RoomClosed":
        return this.#echo(room, event);
      case "MessageActionTriggered":
        return this.#feed.reply(room, `MessageActionTriggered ${event.value.actionId} on ${event.value.messageId}`);
      case "ParticipantActionTriggered":
        return this.#feed.reply(room, `ParticipantActionTriggered ${event.value.actionId} on ${event.value.participantId}`);
      case "ToolbarActionTriggered":
        return this.#feed.reply(room, `ToolbarActionTriggered ${event.value.actionId} on toolbar`);
      case "SuggestionSelected":
        if (event.value.suggestionId === "sg-event") return this.#run(room, helpCommand());
        return;
      case "RoomOpened":
        if (this.#settings(room).activity) {
          try {
            await setActivity(this.#context(), room, true);
          } catch (error) {
            console.error("chat-v2: activity restore failed", error);
          }
        }
        return;
    }
  }

  // ------------------------------------------------------------- renderer

  #render(
    request: ProductRendererRenderRequest,
    send: (node: RendererNode) => void,
    interrupt: (reason?: CallErrorValue<VersionedProductRendererRenderError>) => void,
  ): void {
    void this.ready.then(() => {
      const { context } = request;
      const room = context.tag === "ChatMessage" || context.tag === "ChatFooter" ? context.value.roomId : null;
      this.#record({ via: "render", room, kind: context.tag, data: request });
      if (context.tag === "ChatMessage" && context.value.messageType === CARD_MESSAGE_TYPE) {
        let counter = "?";
        try {
          counter = hexToUtf8(request.payload);
        } catch {
          // A payload that is not UTF-8 shows "?".
        }
        send(cardTree(counter));
      } else if (context.tag === "ChatFooter") {
        send(footerTree());
      } else {
        interrupt({ tag: "HostFailure", value: { reason: `the sample product draws no ${context.tag}` } });
      }
    });
  }

  #rendererAction(item: HostRendererActionSubscribeItem): void {
    void this.ready.then(() => {
      const { context } = item;
      const roomId = context.tag === "ChatMessage" || context.tag === "ChatFooter" ? context.value.roomId : null;
      this.#record({ via: "renderer_action", room: roomId, kind: context.tag, data: item });
      const room = roomId === null ? undefined : knownRoom(roomId);
      if (!room) return;
      if (item.actionId === CARD_LIKE_ACTION) void this.#enqueue(() => this.#feed.reply(room, "card liked"));
      if (item.actionId === FOOTER_PING_ACTION) void this.#enqueue(() => this.#feed.reply(room, "footer ping"));
    });
  }
}

/** Start the sample product. The handlers exist when the function returns. */
export function startSampleProduct(options: SampleProductOptions): SampleProduct {
  return new SampleProduct(options);
}
