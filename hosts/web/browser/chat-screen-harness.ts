/**
 * The Chat screen on its own, for its browser test: the real screen,
 * conversation view and providers, with synthetic data and nothing behind
 * them that reaches a network.
 *
 * - Nova: the real provider over a stand-in engine with one contact and one
 *   request. No keys, statements or chain.
 * - Product rooms: the real provider over the host's TrUAPI Chat store, with a
 *   stand-in worker channel that draws a counter card and echoes text.
 * - Draft: the real draft provider, connected to the synthetic source running
 *   in a real Web Worker, as the host runs it.
 */
import type { HexString } from "@parity/truapi/scale";
import type { RendererNode } from "@parity/truapi";
import type { ProductContext } from "@parity/truapi-host";
import type { ChatSnapshot } from "../src/chat/engine.js";
import type { ChatInboxView, InboxChange } from "../src/chat-inbox.js";
import type { ChatProvider } from "../src/chat-provider.js";
import { ChatScreen } from "../src/chat-screen.js";
import { DraftChatProvider, startDraftWorker } from "../src/draft-chat.js";
import {
  NovaChatProvider,
  type NovaEngine,
} from "../src/nova-chat-provider.js";
import { ProductChat, productChatKey } from "../src/product-chat.js";
import {
  ProductRoomProvider,
  type RoomChannel,
} from "../src/product-room-provider.js";
import { MemoryStorage } from "../src/test-support.js";

const QUIZ = "quiz.test";
const KIM = `0x${"4b".repeat(32)}` as HexString;
const SAM = `0x${"5a".repeat(32)}` as HexString;
const quizWorker: ProductContext = { productId: QUIZ, executionKind: "Worker" };

function novaEngine(): NovaEngine {
  const start = Date.UTC(2026, 9, 20, 9, 0, 0);
  const snapshot: ChatSnapshot = {
    status: { state: "listening" },
    account: `0x${"11".repeat(32)}` as HexString,
    chatKey: `0x${"22".repeat(32)}` as HexString,
    name: "you.01",
    chainKey: "matches",
    canSend: true,
    requests: [
      {
        id: "request-sam",
        peer: SAM,
        peerChatKey: null,
        name: "sam.01",
        welcome: "Hi, it is Sam from the meetup",
        at: start + 60_000,
        version: 2,
        problem: null,
      },
    ],
    contacts: [
      {
        account: KIM,
        chatKey: `0x${"33".repeat(32)}` as HexString,
        name: "kim.01",
        state: "active",
        messages: [
          {
            id: "k1",
            direction: "in",
            kind: "text",
            text: "See you at ten?",
            at: start,
            status: "received",
          },
          {
            id: "k2",
            direction: "out",
            kind: "text",
            text: "Yes, see you",
            at: start + 30_000,
            status: "acknowledged",
          },
        ],
        outbound: null,
        expiry: { request: null, response: null },
        acknowledged: null,
      },
    ],
    skipped: [],
  };
  return {
    snapshot: () => snapshot,
    onChange: () => () => {},
    start: async () => {},
    stop: () => {},
    accept: async () => {},
    decline: () => {},
    startChat: async () => {
      throw new Error("Not in this test.");
    },
    send: async () => {},
    retry: async () => {},
  };
}

const counterCard = (count: number): RendererNode => ({
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
          children: [{ tag: "String", value: { text: `count ${count}` } }],
        },
      },
      {
        tag: "Button",
        value: {
          modifiers: [],
          props: { text: "bump", clickAction: "bump" },
          children: [],
        },
      },
    ],
  },
});

/** A product worker's channel: it draws a counter, counts presses and echoes text. */
function quizChannel(chat: ProductChat): RoomChannel {
  let count = 0;
  const redraws = new Set<() => void>();
  return {
    render: (_request, sink) => {
      const draw = () => sink.onUpdate(counterCard(count));
      redraws.add(draw);
      draw();
      return () => redraws.delete(draw);
    },
    publishRendererAction: async (item) => {
      if (item.actionId !== "bump") return;
      count += 1;
      for (const draw of redraws) draw();
    },
    publishChatAction: async (item) => {
      const payload = item.payload;
      if (payload.tag !== "MessagePosted" || payload.value.tag !== "Text")
        return;
      await chat.platform.postChatMessage(quizWorker, {
        roomId: item.roomId,
        payload: {
          tag: "Text",
          value: { text: `echo: ${payload.value.value.text}` },
        },
      });
    },
  };
}

class TestInbox implements ChatInboxView {
  private list: ChatProvider[] = [];
  private readonly listeners = new Set<(change: InboxChange) => void>();
  draft: DraftChatProvider | null = null;

  signedIn(): boolean {
    return true;
  }

  problem(): string | null {
    return null;
  }

  providers(): readonly ChatProvider[] {
    return this.list;
  }

  onChange(listener: (change: InboxChange) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Start a fresh session: every provider of the last one is disposed first. */
  async session(): Promise<void> {
    for (const provider of this.list) provider.dispose();
    const storage = new MemoryStorage();
    const chat = new ProductChat(storage);
    chat.setScope(productChatKey("test", "wallet"));
    await chat.platform.createChatRoom(quizWorker, {
      roomId: "quiz",
      name: "Quiz bot",
      icon: "",
    });
    await chat.platform.postChatMessage(quizWorker, {
      roomId: "quiz",
      payload: { tag: "Text", value: { text: "Welcome to the quiz" } },
    });
    await chat.platform.postChatMessage(quizWorker, {
      roomId: "quiz",
      payload: {
        tag: "Custom",
        value: { messageType: "quiz.counter", payload: "0x00" },
      },
      alt: "A counter",
    });
    const channel = quizChannel(chat);
    this.draft = new DraftChatProvider({
      id: "draft",
      label: "Draft worker chat",
      connect: startDraftWorker,
      storage,
      cacheKey: "draft-cache",
    });
    this.list = [
      new NovaChatProvider(novaEngine()),
      new ProductRoomProvider(QUIZ, {
        chat,
        channel: () => channel,
        workerState: () => ({
          state: "running",
          manifest: {} as never,
          record: `worker.${QUIZ}`,
          cid: "cid",
        }),
        onWorkersChange: () => () => {},
        archiveImage: async () => null,
        gateway: () => "https://gateway.invalid",
        log: () => {},
      }),
      this.draft,
    ];
    for (const provider of this.list)
      provider.subscribe((event) =>
        this.emit({ type: "provider", providerId: provider.id, event }),
      );
    this.emit({ type: "session" });
  }

  private emit(change: InboxChange): void {
    for (const listener of this.listeners) listener(change);
  }
}

export interface ChatHarness {
  /** A conversation's unread count, as its provider reports it. */
  unread(providerId: string, conversationId: string): number | undefined;
  show(visible: boolean): void;
  restartDraft(): void;
  newSession(): Promise<void>;
}

export async function mountChatScreen(root: HTMLElement): Promise<ChatHarness> {
  const inbox = new TestInbox();
  const screen = new ChatScreen(
    root,
    inbox,
    {
      names: () => [QUIZ],
      state: () => null,
      isDefault: () => false,
      add: async () => {},
      remove: () => {},
      onChange: () => () => {},
    },
    null,
    (pending) => (root.dataset.pending = String(pending)),
  );
  await inbox.session();
  screen.show(true);
  return {
    unread: (providerId, conversationId) =>
      inbox
        .providers()
        .find((provider) => provider.id === providerId)
        ?.conversations()
        .find((summary) => summary.id === conversationId)?.unread,
    show: (visible) => screen.show(visible),
    restartDraft: () => inbox.draft?.restart(),
    newSession: () => inbox.session(),
  };
}
