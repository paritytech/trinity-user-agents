/**
 * The Chat screen on its own, for its browser test: the real screen,
 * conversation view and providers, with synthetic data and nothing behind
 * them that reaches a network.
 *
 * - People: the real Chat Extension v2 draft provider over a stand-in
 *   conversation engine in this page (`serveConversations`), with one
 *   conversation, with Kim, a contact of the host's, and starting chats.
 * - Product rooms: the real provider over the host's TrUAPI Chat store, with a
 *   stand-in worker channel that draws a counter card and echoes text.
 * - Chat product: the real Chat Extension v2 draft provider and host end,
 *   with the synthetic chat product (`fixtures/products/synthetic-chat-product.ts`) on
 *   the other side of a `MessageChannel` in this page. The smoke test runs
 *   the same product as a real product worker.
 */
import type { HexString } from "@parity/truapi/scale";
import { asProductId, type RendererNode } from "@parity/truapi";
import type { ProductContext } from "@parity/truapi-host";
import type { ChatInboxView, InboxChange } from "../../src/chat/inbox.js";
import type { ChatProvider } from "../../src/chat/provider.js";
import { ChatScreen } from "../../src/chat/screen.js";
import { HostContacts, chooseContact } from "../../src/contacts/contacts.js";
import {
  type UsernameDirectory,
  fixtureDirectory,
} from "../../src/contacts/username-directory.js";
import { messagePortBridge } from "../../bridges/chat-v2/contract.js";
import {
  serveConversations,
  type ConversationEngine,
  type ServedConversations,
} from "../../bridges/chat-v2/conversations.js";
import { ChatV2Host } from "../../src/chat/draft/host.js";
import {
  chatV2Over,
  type ChatV2Product,
} from "../../bridges/chat-v2/product.js";
import { ChatV2Provider } from "../../src/chat/draft/provider.js";
import {
  startSyntheticChatProduct,
  type SyntheticChatProduct,
} from "../../fixtures/products/synthetic-chat-product.js";
import { ProductChat, productChatKey } from "../../src/chat/product-chat.js";
import {
  ProductRoomProvider,
  type RoomChannel,
} from "../../src/chat/product-room-provider.js";
import { MemoryStorage } from "../../tests/support/test-support.js";
import {
  WalletContacts,
  walletRecordKey,
} from "../../src/contacts/wallet-contacts.js";

const QUIZ = asProductId("quiz.test");
const CHAT_PRODUCT = "chat.test";
const PEOPLE = "people.test";
const KIM = `0x${"4b".repeat(32)}` as HexString;
const LOU = `0x${"10".repeat(32)}` as HexString;
const ZED = `0x${"7e".repeat(32)}` as HexString;
const EVE = `0x${"e5".repeat(32)}` as HexString;

/**
 * The username index New chat's picker searches: Kim, a contact, and two
 * people who are not. Eve's username names someone else on chain.
 */
const usernames: UsernameDirectory = {
  ...fixtureDirectory("the test index", [
    { account: KIM, username: "kim.01" },
    { account: ZED, username: "zed.01" },
    { account: EVE, username: "eve.02" },
  ]),
  confirm: async ({ username }) =>
    username === "eve.02"
      ? "On chain, eve.02 names another account than the index lists."
      : null,
};
const quizWorker: ProductContext = { productId: QUIZ, executionKind: "Worker" };

/** What the stand-ins were asked to do, for the test to read. */
export const asked: string[] = [];

/**
 * The people product's engine: one conversation, with Kim, by her account,
 * and starts that it records and refuses.
 */
function peopleEngine(): ConversationEngine {
  const start = Date.UTC(2026, 9, 20, 9, 0, 0);
  return {
    conversations: () => [
      {
        id: "kim",
        title: "Kim",
        participants: [{ id: "kim", name: "Kim", account: KIM }],
        composer: { allowed: true },
        contact: { account: KIM, state: "Established" },
      },
    ],
    messages: () => [
      {
        id: "k1",
        author: { kind: "participant", id: "kim" },
        text: "See you at ten?",
        at: start,
        state: "received",
      },
      {
        id: "k2",
        author: { kind: "self" },
        text: "Yes, see you",
        at: start + 30_000,
        state: "delivered",
      },
    ],
    send: async () => {
      throw new Error("Not in this test.");
    },
    retry: async () => {},
    start: async (target, welcome) => {
      asked.push(
        `people start ${"reference" in target ? target.reference : target.handle.bytes} ${welcome}`,
      );
      throw new Error("Not in this test.");
    },
    onChange: () => () => {},
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
      if (payload.tag === "ActionTriggered") {
        await chat.platform.postChatMessage(quizWorker, {
          roomId: item.roomId,
          payload: {
            tag: "Text",
            value: { text: `picked: ${payload.value.actionId}` },
          },
        });
        return;
      }
      if (
        payload.tag === "MessagePosted" &&
        (payload.value.tag === "Reaction" ||
          payload.value.tag === "ReactionRemoved")
      ) {
        asked.push(
          `quiz ${payload.value.tag} ${payload.value.value.messageId} ${payload.value.value.emoji}`,
        );
        return;
      }
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
  private chatV2: ChatV2Host | null = null;
  private product: {
    bridge: ChatV2Product;
    product: SyntheticChatProduct;
  } | null = null;
  private people: ServedConversations | null = null;

  signedIn(): boolean {
    return true;
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
    const question = await chat.platform.postChatMessage(quizWorker, {
      roomId: "quiz",
      payload: {
        tag: "Actions",
        value: {
          text: "Which is a prime?",
          actions: [
            { actionId: "four", title: "4" },
            { actionId: "seven", title: "7" },
          ],
          layout: "Column",
        },
      },
    });
    await chat.platform.postChatMessage(quizWorker, {
      roomId: "quiz",
      payload: {
        tag: "Reaction",
        value: { messageId: question.messageId, emoji: "🤔" },
      },
    });
    await chat.platform.postChatMessage(quizWorker, {
      roomId: "quiz",
      payload: {
        tag: "File",
        value: {
          url: "data:image/png;base64,iVBORw0KGgo=",
          fileName: "answers.png",
          mimeType: "image/png",
          sizeBytes: 2_400_000n,
        },
      },
    });
    const channel = quizChannel(chat);
    this.stopProduct();
    this.people?.stop();
    this.chatV2?.reset();
    this.chatV2 = new ChatV2Host({
      media: () => null,
      contactRooms: (productId) => productId === PEOPLE,
      log: () => {},
    });
    await this.startProduct();
    await this.startPeople();
    this.list = [
      new ChatV2Provider(PEOPLE, this.chatV2, Date.now, {
        names: (account) => (account === KIM ? "kim.01" : null),
        start: {
          target: (account) => ({ contact: account }),
          blocked: () => null,
        },
      }),
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
      new ChatV2Provider(CHAT_PRODUCT, this.chatV2),
    ];
    for (const provider of this.list)
      provider.subscribe((event) =>
        this.emit({ type: "provider", providerId: provider.id, event }),
      );
    this.emit({ type: "session" });
  }

  /** Start the people product's engine behind the bridge, and wait for its rooms. */
  private async startPeople(): Promise<void> {
    const channel = new MessageChannel();
    this.chatV2!.connect(PEOPLE, messagePortBridge(channel.port2));
    this.people = serveConversations(
      chatV2Over(messagePortBridge(channel.port1)),
      peopleEngine(),
      // Kim's message was read before, so no conversation here has unread messages.
      { reads: { get: () => "k2", set: () => {} } },
    );
    await this.people.ready;
  }

  /** Start the chat product, as its worker would start, and wait for its rooms. */
  private async startProduct(): Promise<void> {
    const host = this.chatV2!;
    const channel = new MessageChannel();
    host.connect(CHAT_PRODUCT, messagePortBridge(channel.port2));
    const bridge = chatV2Over(messagePortBridge(channel.port1));
    // Slow enough that a test leaves a room before the answer comes.
    const product = startSyntheticChatProduct(bridge, { stepMs: 1500 });
    this.product = { bridge, product };
    await product.ready;
  }

  private stopProduct(): void {
    this.product?.product.stop();
    this.product?.bridge.close();
    this.product = null;
  }

  /** Stop the chat product and start a fresh one, as a worker restart does. */
  async restartProduct(): Promise<void> {
    this.stopProduct();
    this.chatV2!.disconnect(CHAT_PRODUCT);
    await this.startProduct();
  }

  private emit(change: InboxChange): void {
    for (const listener of this.listeners) listener(change);
  }
}

export interface ChatHarness {
  /** A conversation's unread count, as its provider reports it. */
  unread(providerId: string, conversationId: string): number | undefined;
  show(visible: boolean): void;
  /** Stop the chat product and start a fresh one, as a worker restart does. */
  restartProduct(): Promise<void>;
  newSession(): Promise<void>;
  /** Show a person chosen outside Chat, as the Docklet's search does. */
  startWith(account: HexString, name: string, found: boolean): boolean;
  /** What the stand-in engine and worker were asked, in order. */
  asked(): string[];
  /** How many times the core was told contacts left. */
  removals(): number;
}

/** The wallet's saved chat record: Kim, and Lou, whom the user blocked. */
function walletContacts(): WalletContacts {
  const key = walletRecordKey("test", "wallet");
  const storage = new MemoryStorage();
  const contact = (account: HexString, name: string, blocked: boolean) => ({
    account,
    chatKey: `0x${"cc".repeat(32)}`,
    name,
    state: "active",
    messages: [],
    outbound: null,
    expiry: { request: null, response: null },
    acknowledged: null,
    ...(blocked ? { blocked } : {}),
  });
  storage.setItem(
    key,
    JSON.stringify({
      contacts: [contact(KIM, "kim.01", false), contact(LOU, "lou.01", true)],
      requests: [],
      declined: [],
      left: [],
    }),
  );
  const contacts = new WalletContacts(storage);
  contacts.setSession({ key, scope: key, offered: () => true });
  return contacts;
}

export async function mountChatScreen(root: HTMLElement): Promise<ChatHarness> {
  const inbox = new TestInbox();
  const saved = walletContacts();
  let removals = 0;
  const people = new HostContacts({
    list: () => saved.contacts(),
    onChange: (listener) => saved.onChange(listener),
    choose: (asker, contacts, blocked) =>
      chooseContact(asker, contacts, usernames, blocked),
    blocked: () => saved.blocked(),
    onSessionChange: (listener) =>
      inbox.onChange((change) => {
        if (change.type === "session") listener();
      }),
    // The core is told a contact left: their handle stops resolving.
    removed: () => (removals += 1),
    // No core runs here, so the host names no contact handle.
    mint: async () => undefined,
  });
  const screen = new ChatScreen(
    root,
    inbox,
    () => people.choose({ title: "New chat" }),
    undefined,
    {
      list: () => saved.blockedPeople(),
      blockable: () => saved.blockable(),
      choose: () =>
        chooseContact(
          { title: "Block a contact" },
          saved.blockable(),
          fixtureDirectory("your contacts", []),
        ),
      block: (account) => saved.block(account),
      unblock: (account) => saved.unblock(account),
      onChange: (listener) => saved.onChange(listener),
    },
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
    restartProduct: () => inbox.restartProduct(),
    newSession: () => inbox.session(),
    startWith: (account, name, found) =>
      screen.startWith({ account, name, found }),
    asked: () => [...asked],
    removals: () => removals,
  };
}
