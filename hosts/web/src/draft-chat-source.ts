import type { RendererNode } from "@parity/truapi";
import {
  DRAFT_PROTOCOL,
  type DraftMessage,
  type DraftParticipant,
  type DraftReplies,
  type DraftRequest,
  type DraftSummary,
  type SourcePort,
  type WorkerToHost,
} from "./draft-chat.js";

/**
 * DRAFT, NOT TRUAPI. The synthetic worker behind the draft chat channel: a
 * stand-in for a chat product that owns its protocol and history. It keeps
 * three conversations in memory, with made-up people and text, and plays the
 * network: a message it is handed goes from pending to sent to delivered, a
 * peer answers in a direct conversation and reads what it answers, and a
 * message with `[fail]` in it fails once. Nothing leaves the worker.
 */

const SELF = "me";

const PEOPLE: DraftParticipant[] = [
  { id: "ada", label: "Ada" },
  { id: "grace", label: "Grace" },
  { id: "lin", label: "Lin" },
];

interface Conversation {
  id: string;
  title: string;
  kind: "direct" | "group";
  /** The peer who answers, in a direct conversation. */
  peer: string | null;
  messages: DraftMessage[];
  /** The newest message the user has seen. */
  readThrough: string | null;
}

export interface DraftSourceOptions {
  now?(): number;
  /** How the source waits; tests pass one they can run by hand. */
  schedule?(task: () => void, delayMs: number): void;
  /** The gap between one status and the next. */
  stepMs?: number;
}

const PHRASES = [
  "Did the build pass?",
  "Yes, all green.",
  "Can you look at the layout?",
  "Sure, after lunch.",
  "The page loads faster now.",
  "Nice. Ship it?",
];

export class DraftChatSource {
  private readonly conversations = new Map<string, Conversation>();
  private readonly renders = new Map<
    number,
    { conversationId: string; messageId: string }
  >();
  private readonly votes = new Map<string, number>();
  private readonly failedOnce = new Set<string>();
  private readonly now: () => number;
  private readonly schedule: (task: () => void, delayMs: number) => void;
  private readonly stepMs: number;
  private sent = 0;

  constructor(
    private readonly port: SourcePort,
    options: DraftSourceOptions = {},
  ) {
    this.now = options.now ?? Date.now;
    this.schedule =
      options.schedule ?? ((task, delayMs) => void setTimeout(task, delayMs));
    this.stepMs = options.stepMs ?? 400;
    this.seed();
    port.listen((message) => {
      if (message.protocol !== DRAFT_PROTOCOL) return;
      try {
        this.reply(message.id, this.handle(message.request));
      } catch (error) {
        this.post({
          kind: "reply",
          id: message.id,
          ok: false,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    });
  }

  private seed(): void {
    const start = this.now() - 3 * 24 * 60 * 60 * 1000;
    const text = (n: number) => `#${n} ${PHRASES[n % PHRASES.length]}`;
    const history = (
      id: string,
      count: number,
      senders: string[],
      spacingMs: number,
    ): DraftMessage[] =>
      Array.from({ length: count }, (_, index) => ({
        id: `${id}-${index + 1}`,
        at: start + index * spacingMs,
        senderId: senders[index % senders.length],
        status: senders[index % senders.length] === SELF ? "read" : "delivered",
        content: { tag: "Text", text: text(index + 1) },
      }));

    const ada = history("ada", 120, ["ada", SELF], 60_000);
    const design = history("design", 9, ["ada", "grace", SELF], 3_600_000);
    design.push({
      id: "design-poll",
      at: start + 10 * 3_600_000,
      senderId: "grace",
      status: "delivered",
      content: {
        tag: "Custom",
        messageType: "draft.poll",
        payload: "0x00",
        alt: "A poll: lunch at noon?",
      },
    });
    const lin = history("lin", 7, ["lin", SELF], 600_000).map((message) => ({
      ...message,
      at: message.at + 2 * 24 * 60 * 60 * 1000,
    }));
    for (const conversation of [
      { id: "ada", title: "Ada", kind: "direct", peer: "ada", messages: ada },
      {
        id: "design",
        title: "Design group",
        kind: "group",
        peer: null,
        messages: design,
      },
      { id: "lin", title: "Lin", kind: "direct", peer: "lin", messages: lin },
    ] as const) {
      const messages = [...conversation.messages];
      // The user has not seen Lin's last three messages.
      const seen = conversation.id === "lin" ? 1 : messages.length - 1;
      this.conversations.set(conversation.id, {
        ...conversation,
        messages,
        readThrough: messages[seen]?.id ?? null,
      });
    }
  }

  private post(
    message: { kind: WorkerToHost["kind"] } & Record<string, unknown>,
  ): void {
    this.port.post({ protocol: DRAFT_PROTOCOL, ...message } as WorkerToHost);
  }

  private reply(id: number, value: unknown): void {
    this.post({ kind: "reply", id, ok: true, value });
  }

  private conversation(id: string): Conversation {
    const conversation = this.conversations.get(id);
    if (conversation === undefined) throw new Error(`No conversation ${id}.`);
    return conversation;
  }

  private summaries(): DraftSummary[] {
    return [...this.conversations.values()].map((conversation) => {
      const last = conversation.messages.at(-1);
      const seenAt = conversation.messages.findIndex(
        (message) => message.id === conversation.readThrough,
      );
      const author =
        last === undefined || last.senderId === SELF
          ? "You: "
          : conversation.kind === "group"
            ? `${PEOPLE.find((person) => person.id === last.senderId)?.label}: `
            : "";
      return {
        id: conversation.id,
        title: conversation.title,
        kind: conversation.kind,
        preview:
          last === undefined
            ? ""
            : `${author}${last.content.tag === "Text" ? last.content.text : (last.content.alt ?? "A message")}`,
        updatedAt: last?.at ?? 0,
        unread: conversation.messages
          .slice(seenAt + 1)
          .filter((message) => message.senderId !== SELF).length,
      };
    });
  }

  private pushSummaries(): void {
    this.post({ kind: "summaries", conversations: this.summaries() });
  }

  private feed(conversationId: string, message: DraftMessage): void {
    this.post({ kind: "feed", conversationId, message: { ...message } });
  }

  private handle(request: DraftRequest): DraftReplies[keyof DraftReplies] {
    switch (request.kind) {
      case "hello":
        return { selfId: SELF, conversations: this.summaries() };
      case "page":
        return this.page(request.conversationId, request.cursor, request.limit);
      case "participants":
        return {
          participants: PEOPLE.filter((person) =>
            request.ids.includes(person.id),
          ),
        };
      case "send":
        return { message: this.send(request.conversationId, request.text) };
      case "retry":
        return {
          message: this.retry(request.conversationId, request.messageId),
        };
      case "seen":
        this.seen(request.conversationId, request.throughMessageId);
        return {};
      case "render":
        this.renders.set(request.renderId, request);
        this.drawPoll(request.renderId);
        return {};
      case "stopRender":
        this.renders.delete(request.renderId);
        return {};
      case "action":
        if (request.actionId === "vote") {
          this.votes.set(
            request.messageId,
            (this.votes.get(request.messageId) ?? 0) + 1,
          );
          for (const [renderId, render] of this.renders)
            if (render.messageId === request.messageId) this.drawPoll(renderId);
        }
        return {};
    }
  }

  /** Pages end just before the cursor's message; the cursor format is the worker's own. */
  private page(
    conversationId: string,
    cursor: string | undefined,
    limit: number,
  ): DraftReplies["page"] {
    const messages = this.conversation(conversationId).messages;
    const end =
      cursor === undefined
        ? messages.length
        : messages.findIndex(
            (message) => `older-than:${message.id}` === cursor,
          );
    if (end < 0) throw new Error("That page is gone.");
    const start = Math.max(0, end - limit);
    const page = messages.slice(start, end).map((message) => ({ ...message }));
    return start > 0
      ? { messages: page, cursor: `older-than:${messages[start].id}` }
      : { messages: page };
  }

  private send(conversationId: string, text: string): DraftMessage {
    const conversation = this.conversation(conversationId);
    const message: DraftMessage = {
      id: `${conversationId}-you-${++this.sent}`,
      at: this.now(),
      senderId: SELF,
      status: "pending",
      content: { tag: "Text", text },
    };
    conversation.messages.push(message);
    conversation.readThrough = message.id;
    this.pushSummaries();
    this.deliver(conversation, message);
    return { ...message };
  }

  private retry(conversationId: string, messageId: string): DraftMessage {
    const conversation = this.conversation(conversationId);
    const message = conversation.messages.find(
      (entry) => entry.id === messageId,
    );
    if (message === undefined || message.status !== "failed")
      throw new Error("Only a failed message can be sent again.");
    message.status = "pending";
    delete message.problem;
    this.deliver(conversation, message);
    return { ...message };
  }

  /** Play the network: pending, then sent, then delivered, or failed once for `[fail]`. */
  private deliver(conversation: Conversation, message: DraftMessage): void {
    const text = message.content.tag === "Text" ? message.content.text : "";
    this.schedule(() => {
      if (text.includes("[fail]") && !this.failedOnce.has(message.id)) {
        this.failedOnce.add(message.id);
        message.status = "failed";
        message.problem =
          "The synthetic network dropped it. Retry sends it again.";
        this.feed(conversation.id, message);
        return;
      }
      message.status = "sent";
      this.feed(conversation.id, message);
      this.schedule(() => {
        message.status = "delivered";
        this.feed(conversation.id, message);
        if (conversation.peer !== null)
          this.schedule(
            () => this.answer(conversation, message, text),
            this.stepMs,
          );
      }, this.stepMs);
    }, this.stepMs);
  }

  /** The peer reads the user's messages and answers. */
  private answer(
    conversation: Conversation,
    to: DraftMessage,
    text: string,
  ): void {
    for (const message of conversation.messages)
      if (message.senderId === SELF && message.status === "delivered") {
        message.status = "read";
        this.feed(conversation.id, message);
      }
    const answer: DraftMessage = {
      id: `${to.id}-answer`,
      at: this.now(),
      senderId: conversation.peer!,
      status: "delivered",
      content: { tag: "Text", text: `Got it: "${text}"` },
    };
    conversation.messages.push(answer);
    this.feed(conversation.id, answer);
    this.pushSummaries();
  }

  private seen(conversationId: string, throughMessageId: string): void {
    const conversation = this.conversation(conversationId);
    const index = conversation.messages.findIndex(
      (message) => message.id === throughMessageId,
    );
    const current = conversation.messages.findIndex(
      (message) => message.id === conversation.readThrough,
    );
    if (index <= current) return;
    conversation.readThrough = throughMessageId;
    this.pushSummaries();
  }

  private drawPoll(renderId: number): void {
    const render = this.renders.get(renderId);
    if (render === undefined) return;
    const votes = this.votes.get(render.messageId) ?? 0;
    const tree: RendererNode = {
      tag: "Column",
      value: {
        modifiers: [],
        props: { horizontalAlignment: "Start" },
        children: [
          {
            tag: "Text",
            value: {
              modifiers: [],
              props: { style: "TitleMediumRegular" },
              children: [{ tag: "String", value: { text: "Lunch at noon?" } }],
            },
          },
          {
            tag: "Text",
            value: {
              modifiers: [],
              props: { style: "BodySmallRegular" },
              children: [
                {
                  tag: "String",
                  value: { text: `${votes} ${votes === 1 ? "vote" : "votes"}` },
                },
              ],
            },
          },
          {
            tag: "Button",
            value: {
              modifiers: [],
              props: { text: "Vote", clickAction: "vote" },
              children: [],
            },
          },
        ],
      },
    };
    this.post({ kind: "tree", renderId, tree });
  }
}
