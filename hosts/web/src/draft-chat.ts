import type { HexString, RendererNode } from "@parity/truapi";
import { bytesToHex } from "@parity/truapi/scale";
import {
  Listeners,
  PAGE_SIZE,
  type ChatMessage,
  type ChatProvider,
  type Composer,
  type ConversationSummary,
  type CustomView,
  type ProviderEvent,
  type ProviderStatus,
  type TimelinePage,
} from "./chat-provider.js";

/**
 * DRAFT, NOT TRUAPI. A worker-owned chat source, shaped after the Chat
 * Extension v2 RFC draft (RFC 0002, paritytech/trinity-user-agents PR 13,
 * closed unmerged; tracking issue 487). It exists to try that draft's
 * product-as-message-source model against a real host screen, and is not a
 * protocol any host or product ships.
 *
 * The worker owns its conversations, history, delivery and read state. The
 * host asks it for pages and participants, hands it what the user sends and
 * what the user has seen, and keeps only a cache of the conversation list.
 * Requests and their replies travel over a page-local `postMessage` channel
 * to an ordinary Web Worker, not over TrUAPI frames, so nothing here reaches
 * the core or the inspector.
 *
 * Where this departs from the RFC draft, it is because the host screen
 * needed it (listed again in the README's Chat section):
 * - `hello` names the user's own participant id, which the draft does not;
 * - participants carry their id, so `participants` replies can be matched;
 * - a status change is an upsert of the same message id on the feed, as the
 *   draft has no status update;
 * - `seen` takes a "seen through" message, not one call per message;
 * - `Custom` messages are drawn by the worker as `RendererNode` trees, the
 *   shipped `Renderer` model, not the draft's `SerializedCustomChatMessage`;
 * - every host-to-worker call is a request with a correlated reply, which the
 *   draft's subscription-only transport cannot express.
 */

export const DRAFT_PROTOCOL = "draft-chat/0";

export type DraftStatus = "pending" | "sent" | "delivered" | "read" | "failed";

export interface DraftMessage {
  id: string;
  at: number;
  senderId: string;
  status: DraftStatus;
  problem?: string;
  content:
    | { tag: "Text"; text: string }
    | {
        tag: "Custom";
        messageType: string;
        payload: HexString;
        alt: string | null;
      };
}

export interface DraftSummary {
  id: string;
  title: string;
  kind: "direct" | "group";
  preview: string;
  updatedAt: number;
  unread: number;
}

export interface DraftParticipant {
  id: string;
  label: string;
}

/** Host-to-worker calls, each answered by one reply. */
export type DraftRequest =
  | { kind: "hello" }
  | { kind: "page"; conversationId: string; cursor?: string; limit: number }
  | { kind: "participants"; ids: string[] }
  | { kind: "send"; conversationId: string; text: string }
  | { kind: "retry"; conversationId: string; messageId: string }
  | { kind: "seen"; conversationId: string; throughMessageId: string }
  | {
      kind: "render";
      renderId: number;
      conversationId: string;
      messageId: string;
    }
  | { kind: "stopRender"; renderId: number }
  | {
      kind: "action";
      conversationId: string;
      messageId: string;
      actionId: string;
      payload: HexString;
    };

/** What each call replies with. */
export interface DraftReplies {
  hello: { selfId: string; conversations: DraftSummary[] };
  page: { messages: DraftMessage[]; cursor?: string };
  participants: { participants: DraftParticipant[] };
  send: { message: DraftMessage };
  retry: { message: DraftMessage };
  seen: Record<string, never>;
  render: Record<string, never>;
  stopRender: Record<string, never>;
  action: Record<string, never>;
}

export interface HostToWorker {
  protocol: typeof DRAFT_PROTOCOL;
  id: number;
  request: DraftRequest;
}

export type WorkerToHost = { protocol: typeof DRAFT_PROTOCOL } & (
  | { kind: "reply"; id: number; ok: true; value: unknown }
  | { kind: "reply"; id: number; ok: false; error: string }
  /** The conversation list changed: names, previews, order or unread counts. */
  | { kind: "summaries"; conversations: DraftSummary[] }
  /** A message was added or changed, keyed by its id. */
  | { kind: "feed"; conversationId: string; message: DraftMessage }
  /** A new tree for an open render. */
  | { kind: "tree"; renderId: number; tree: RendererNode }
);

/** The host's end of the channel. */
export interface DraftPort {
  post(message: HostToWorker): void;
  listen(handler: (message: WorkerToHost) => void): void;
  close(): void;
}

/** The worker's end of the channel. */
export interface SourcePort {
  post(message: WorkerToHost): void;
  listen(handler: (message: HostToWorker) => void): void;
}

/** The host's end of a channel to a Web Worker. */
export function workerPort(worker: Worker): DraftPort {
  return {
    post: (message) => worker.postMessage(message),
    listen: (handler) => {
      worker.onmessage = (event: MessageEvent<WorkerToHost>) => {
        if (event.data?.protocol === DRAFT_PROTOCOL) handler(event.data);
      };
    },
    close: () => worker.terminate(),
  };
}

/** Start the synthetic fixture in a Web Worker of its own. */
export function startDraftWorker(): DraftPort {
  return workerPort(
    new Worker(new URL("./draft-chat-worker.ts", import.meta.url), {
      type: "module",
      name: "draft chat fixture",
    }),
  );
}

/** Where the host keeps the draft worker's last conversation list. */
export const draftCacheKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:chat-draft:${walletId}:v1`;

/** Where the host keeps whether the user turned the draft worker on. */
export const draftEnabledKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:chat-draft-enabled:${walletId}:v1`;

export interface DraftChatOptions {
  id: string;
  label: string;
  connect(): DraftPort;
  storage: Storage;
  /** Where the last conversation list is kept, to show before the worker answers. */
  cacheKey: string;
  timeoutMs?: number;
  log?(text: string): void;
}

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
  timer: ReturnType<typeof setTimeout>;
}

interface OpenRender {
  view: CustomView;
  conversationId: string;
  messageId: string;
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class DraftChatProvider implements ChatProvider {
  readonly id: string;
  readonly label: string;
  private readonly listeners = new Listeners<ProviderEvent>();
  private port: DraftPort | null = null;
  private generation = 0;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly renders = new Map<number, OpenRender>();
  private state: ProviderStatus = { state: "starting" };
  private summaries: DraftSummary[];
  private selfId: string | null = null;
  private readonly labels = new Map<string, string>();

  constructor(private readonly options: DraftChatOptions) {
    this.id = options.id;
    this.label = options.label;
    this.summaries = this.loadCache();
    this.connect();
  }

  private loadCache(): DraftSummary[] {
    try {
      const cached: unknown = JSON.parse(
        this.options.storage.getItem(this.options.cacheKey) ?? "[]",
      );
      return Array.isArray(cached) ? (cached as DraftSummary[]) : [];
    } catch {
      return [];
    }
  }

  private connect(): void {
    const port = this.options.connect();
    const epoch = this.generation;
    this.port = port;
    port.listen((message) => {
      if (this.port === port) this.receive(message);
    });
    this.request("hello", { kind: "hello" })
      .then((hello) => {
        if (this.generation !== epoch) return;
        this.selfId = hello.selfId;
        this.setSummaries(hello.conversations);
        this.state = { state: "ready" };
        this.listeners.emit({ type: "status" });
      })
      .catch((error: unknown) => {
        if (this.generation !== epoch) return;
        this.state = { state: "failed", reason: errorText(error) };
        this.listeners.emit({ type: "status" });
      });
  }

  /**
   * Stop the worker and start a fresh one. Replies the old worker still owes
   * are refused, and open renders are told to draw again.
   */
  restart(): void {
    this.disconnect("The draft worker restarted.");
    this.generation += 1;
    this.state = { state: "starting" };
    this.connect();
    this.listeners.emit({ type: "status" });
  }

  private disconnect(reason: string): void {
    this.port?.close();
    this.port = null;
    this.selfId = null;
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error(reason));
    }
    this.pending.clear();
    for (const render of this.renders.values()) render.view.fail(reason);
    this.renders.clear();
  }

  private request<K extends keyof DraftReplies>(
    kind: K,
    request: Extract<DraftRequest, { kind: K }>,
  ): Promise<DraftReplies[K]> {
    const port = this.port;
    if (port === null)
      return Promise.reject(new Error("The draft worker is not running."));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`The draft worker did not answer ${kind}.`));
      }, this.options.timeoutMs ?? 10_000);
      this.pending.set(id, {
        resolve: (value) => resolve(value as DraftReplies[K]),
        reject,
        timer,
      });
      port.post({ protocol: DRAFT_PROTOCOL, id, request });
    });
  }

  private receive(message: WorkerToHost): void {
    switch (message.kind) {
      case "reply": {
        const pending = this.pending.get(message.id);
        if (pending === undefined) return;
        this.pending.delete(message.id);
        clearTimeout(pending.timer);
        if (message.ok) pending.resolve(message.value);
        else pending.reject(new Error(message.error));
        return;
      }
      case "summaries":
        this.setSummaries(message.conversations);
        return;
      case "feed": {
        const epoch = this.generation;
        void this.ensureLabels([message.message.senderId]).then(() => {
          if (this.generation !== epoch) return;
          this.listeners.emit({
            type: "messages",
            conversationId: message.conversationId,
            message: this.message(message.message),
          });
        });
        return;
      }
      case "tree": {
        const render = this.renders.get(message.renderId);
        render?.view.draw(message.tree, {
          action: (actionId, payload) =>
            void this.request("action", {
              kind: "action",
              conversationId: render.conversationId,
              messageId: render.messageId,
              actionId,
              payload: bytesToHex(payload) as HexString,
            }).catch((error: unknown) =>
              this.options.log?.(
                `draft chat: action ${actionId} not delivered: ${errorText(error)}`,
              ),
            ),
          image: () => null,
        });
        return;
      }
    }
  }

  private setSummaries(conversations: DraftSummary[]): void {
    this.summaries = conversations;
    this.options.storage.setItem(
      this.options.cacheKey,
      JSON.stringify(conversations),
    );
    this.listeners.emit({ type: "conversations" });
  }

  /** Ask the worker for the names of senders the host has not seen yet. */
  private async ensureLabels(ids: string[]): Promise<void> {
    const unknown = [...new Set(ids)].filter(
      (id) => id !== this.selfId && !this.labels.has(id),
    );
    if (unknown.length === 0) return;
    const { participants } = await this.request("participants", {
      kind: "participants",
      ids: unknown,
    });
    for (const participant of participants)
      this.labels.set(participant.id, participant.label);
  }

  private message(message: DraftMessage): ChatMessage {
    const self = message.senderId === this.selfId;
    return {
      id: message.id,
      at: message.at,
      sender: {
        id: message.senderId,
        label: self
          ? "You"
          : (this.labels.get(message.senderId) ?? message.senderId),
        self,
      },
      status: self ? message.status : "received",
      ...(message.problem === undefined ? {} : { problem: message.problem }),
      content: message.content,
    };
  }

  status(): ProviderStatus {
    return this.state;
  }

  epoch(): number {
    return this.generation;
  }

  conversations(): ConversationSummary[] {
    return this.summaries.map((summary) => ({
      id: summary.id,
      title: summary.title,
      subtitle: `${summary.kind === "group" ? "Group" : "Direct"} · draft worker, not TrUAPI`,
      icon: null,
      seed: summary.id,
      preview: summary.preview,
      updatedAt: summary.updatedAt,
      unread: summary.unread,
    }));
  }

  async timeline(
    conversationId: string,
    before?: string,
  ): Promise<TimelinePage> {
    const page = await this.request("page", {
      kind: "page",
      conversationId,
      limit: PAGE_SIZE,
      ...(before === undefined ? {} : { cursor: before }),
    });
    await this.ensureLabels(page.messages.map((message) => message.senderId));
    return {
      messages: page.messages.map((message) => this.message(message)),
      ...(page.cursor === undefined ? {} : { before: page.cursor }),
    };
  }

  composer(): Composer {
    if (this.state.state === "ready") return { kind: "text" };
    return {
      kind: "blocked",
      reason:
        this.state.state === "failed"
          ? `The draft worker failed: ${this.state.reason}`
          : "The draft worker is starting.",
    };
  }

  async send(conversationId: string, text: string): Promise<void> {
    const { message } = await this.request("send", {
      kind: "send",
      conversationId,
      text,
    });
    this.listeners.emit({
      type: "messages",
      conversationId,
      message: this.message(message),
    });
  }

  async retry(conversationId: string, messageId: string): Promise<void> {
    const { message } = await this.request("retry", {
      kind: "retry",
      conversationId,
      messageId,
    });
    this.listeners.emit({
      type: "messages",
      conversationId,
      message: this.message(message),
    });
  }

  markRead(conversationId: string, throughMessageId: string): void {
    void this.request("seen", {
      kind: "seen",
      conversationId,
      throughMessageId,
    }).catch((error: unknown) =>
      this.options.log?.(`draft chat: seen not delivered: ${errorText(error)}`),
    );
  }

  renderCustom(
    conversationId: string,
    message: ChatMessage,
    view: CustomView,
  ): () => void {
    const renderId = this.nextId++;
    this.renders.set(renderId, {
      view,
      conversationId,
      messageId: message.id,
    });
    this.request("render", {
      kind: "render",
      renderId,
      conversationId,
      messageId: message.id,
    }).catch((error: unknown) => {
      if (this.renders.delete(renderId))
        view.fail(`Could not draw this message: ${errorText(error)}`);
    });
    return () => {
      if (!this.renders.delete(renderId)) return;
      void this.request("stopRender", { kind: "stopRender", renderId }).catch(
        () => {},
      );
    };
  }

  subscribe(listener: (event: ProviderEvent) => void): () => void {
    return this.listeners.add(listener);
  }

  dispose(): void {
    this.generation += 1;
    this.disconnect("The draft chat stopped.");
    this.listeners.clear();
  }
}
