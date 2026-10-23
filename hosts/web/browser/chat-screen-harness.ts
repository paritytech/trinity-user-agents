/**
 * The Chat screen on its own, for its browser test: the real screen,
 * conversation view and providers, with synthetic data and nothing behind
 * them that reaches a network.
 *
 * - Nova: the real provider over a stand-in engine with one contact and one
 *   request. No keys, statements or chain.
 * - Product rooms: the real provider over the host's TrUAPI Chat store, with a
 *   stand-in worker channel that draws a counter card and echoes text.
 * - Chat product: the real Chat Extension v2 draft provider and host end,
 *   with the synthetic chat product (`browser/synthetic-chat-product.ts`) on
 *   the other side of a `MessageChannel` in this page. The smoke test runs
 *   the same product as a real product worker.
 */
import type { HexString } from "@parity/truapi/scale";
import type { RendererNode } from "@parity/truapi";
import type { ProductContext } from "@parity/truapi-host";
import type { ChatSnapshot, IncomingCallSignal } from "../src/chat/engine.js";
import { CallManager } from "../src/chat/call.js";
import { decodeMessage, encodeMessage } from "../src/chat/protocol.js";
import type { ChatInboxView, InboxChange } from "../src/chat-inbox.js";
import type { ChatProvider } from "../src/chat-provider.js";
import { ChatScreen } from "../src/chat-screen.js";
import { messagePortBridge } from "../src/chat-v2/contract.js";
import { ChatV2Host } from "../src/chat-v2/host.js";
import { chatV2Over, type ChatV2Product } from "../src/chat-v2/product.js";
import { ChatV2Provider } from "../src/chat-v2/provider.js";
import {
  startSyntheticChatProduct,
  type SyntheticChatProduct,
} from "./synthetic-chat-product.js";
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
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import * as sr25519 from "@scure/sr25519";
import { cryptoRandom } from "../src/chat/crypto.js";
import { MemoryHopNode } from "../src/chat/hop-memory.js";
import { hopDownload, hopUpload } from "../src/chat/hop.js";

/** The HOP node files go through in this page: in memory, with real sealing and checks. */
const hopNode = new MemoryHopNode();
const HOP_URL = "wss://hop.invalid";
const hopSecret = sr25519.secretFromSeed(new Uint8Array(32).fill(5));

const QUIZ = "quiz.test";
const CHAT_PRODUCT = "chat.test";
const KIM = `0x${"4b".repeat(32)}` as HexString;
const SAM = `0x${"5a".repeat(32)}` as HexString;
const quizWorker: ProductContext = { productId: QUIZ, executionKind: "Worker" };

/** What the stand-ins were asked to do, for the test to read. */
export const asked: string[] = [];

const YOU = `0x${"11".repeat(32)}` as HexString;
/**
 * Generated media in place of a camera and microphone: a tone from WebAudio
 * and a canvas whose color changes. No device is opened.
 */
async function media(video: boolean): Promise<MediaStream> {
  const audio = new AudioContext();
  const tone = audio.createOscillator();
  const out = audio.createMediaStreamDestination();
  tone.connect(out);
  tone.start();
  const tracks: MediaStreamTrack[] = out.stream.getAudioTracks();
  if (video) {
    const canvas = document.createElement("canvas");
    canvas.width = 160;
    canvas.height = 120;
    const paint = canvas.getContext("2d")!;
    let hue = 0;
    setInterval(() => {
      paint.fillStyle = `hsl(${(hue += 7) % 360} 70% 50%)`;
      paint.fillRect(0, 0, 160, 120);
    }, 60);
    tracks.push(...canvas.captureStream(15).getVideoTracks());
  }
  return new MediaStream(tracks);
}

/** A call signal as it would arrive: encoded and decoded through the chat wire. */
function overWire(
  account: HexString,
  id: string,
  signal: Parameters<CallManager["receive"]>[0]["content"],
): IncomingCallSignal {
  const at = Date.now();
  const content = decodeMessage(
    encodeMessage({ id, timestampMs: BigInt(at), content: signal }),
  ).content as IncomingCallSignal["content"];
  return { account, id, at, content };
}

/**
 * Kim's side of calls: a real call manager in this page, with
 * generated media. It answers every call it gets.
 */
const callListeners = new Set<(signal: IncomingCallSignal) => void>();
let nextCall = 1;
export const kim = new CallManager({
  send: async (_account, signal) => {
    const id = `kim-call-${nextCall++}`;
    asked.push(`kim ${signal.kind}`);
    const delivered = overWire(KIM, id, signal);
    queueMicrotask(() => {
      for (const listener of callListeners) listener(delivered);
    });
    return id;
  },
  media,
  rtc: { iceServers: [] },
  now: Date.now,
});
let kimAnswers = true;
kim.subscribe(() => {
  const held = kim.current();
  if (kimAnswers && held?.session.state === "incoming")
    void held.session.accept();
});

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
  const listeners = new Set<() => void>();
  const change = () => {
    for (const listener of listeners) listener();
  };
  return {
    snapshot: () => snapshot,
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    start: async () => {},
    stop: () => {},
    accept: async () => {},
    decline: () => {},
    startChat: async () => {
      throw new Error("Not in this test.");
    },
    send: async () => {},
    retry: async () => {},
    react: async (account, messageId, emoji) => {
      asked.push(`nova react ${messageId} ${emoji}`);
      const target = snapshot.contacts
        .find((contact) => contact.account === account)
        ?.messages.find((message) => message.id === messageId);
      if (target === undefined) return;
      const mine = target.reactions?.find((entry) => entry.from === "self");
      target.reactions = [
        ...(target.reactions ?? []).filter((entry) => entry.from !== "self"),
        ...(emoji === null || emoji === mine?.emoji
          ? []
          : [{ emoji, from: "self" as const }]),
      ];
      change();
    },
    block: async (account) => {
      asked.push(`nova block`);
      const contact = snapshot.contacts.find((c) => c.account === account);
      if (contact) contact.blocked = true;
      change();
    },
    unblock: async (account) => {
      asked.push(`nova unblock`);
      const contact = snapshot.contacts.find((c) => c.account === account);
      if (contact) delete contact.blocked;
      change();
    },
    fileProblem: () => null,
    fetchProblem: (attachment) =>
      attachment.identifier === undefined ? "Not uploaded." : null,
    sendFile: async (account, file, caption) => {
      asked.push(`nova file ${file.name} ${file.bytes.length}`);
      const contact = snapshot.contacts.find((c) => c.account === account)!;
      const message = {
        id: `file-${contact.messages.length}`,
        direction: "out" as const,
        kind: "text" as const,
        text: caption,
        at: Date.now(),
        status: "pending" as const,
        attachments: [
          {
            node: HOP_URL,
            kind:
              file.image === undefined ? ("file" as const) : ("image" as const),
            mimeType: file.mimeType,
            size: file.bytes.length,
            ...file.image,
            name: file.name,
          },
        ],
      };
      contact.messages.push(message);
      change();
      const sent = await hopUpload(
        hopNode.connect(),
        file.bytes,
        {
          publicKey: sr25519.getPublicKey(hopSecret),
          sign: (payload) => sr25519.sign(hopSecret, payload),
        },
        { random: cryptoRandom, now: Date.now },
      );
      Object.assign(message.attachments[0], {
        identifier: bytesToHex(sent.identifier),
        ticket: bytesToHex(sent.ticket),
      });
      Object.assign(message, { status: "acknowledged" });
      change();
    },
    cancel: () => {},
    fetchFile: async (account, messageId, index) => {
      asked.push(`nova fetch ${messageId}`);
      const attachment = snapshot.contacts
        .find((c) => c.account === account)!
        .messages.find((m) => m.id === messageId)!.attachments![index];
      return hopDownload(
        hopNode.connect(),
        {
          identifier: hexToBytes(attachment.identifier!),
          ticket: hexToBytes(attachment.ticket!),
        },
        1024 * 1024,
      );
    },
    sendCall: async (account, signal) => {
      const id = `you-call-${nextCall++}`;
      asked.push(`nova ${signal.kind}`);
      if (signal.kind === "call-offer") {
        snapshot.contacts
          .find((contact) => contact.account === account)
          ?.messages.push({
            id,
            direction: "out",
            kind: "call",
            text: "",
            at: Date.now(),
            status: "acknowledged",
            call: { video: signal.video },
          });
        change();
      }
      const delivered = overWire(YOU, id, signal);
      queueMicrotask(() => kim.receive(delivered));
      return id;
    },
    onCall: (listener) => {
      callListeners.add(listener);
      return () => callListeners.delete(listener);
    },
    leave: async (account) => {
      asked.push(`nova leave`);
      snapshot.contacts = snapshot.contacts.filter(
        (contact) => contact.account !== account,
      );
      change();
    },
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
    this.chatV2?.reset();
    this.chatV2 = new ChatV2Host({ media: () => null, log: () => {} });
    await this.startProduct();
    this.list = [
      new NovaChatProvider(novaEngine(), undefined, {
        media,
        rtc: { iceServers: [] },
        now: Date.now,
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
  /** What the stand-in engine and worker were asked, in order. */
  asked(): string[];
  /** Kim's side of the call, as it stands. */
  kimCall(): { state: string; tracks: string[] } | null;
  /** Kim calls the user, and does not answer calls until the next session. */
  kimCalls(video: boolean): Promise<void>;
  /** Kim hangs up. */
  kimHangsUp(): void;
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
    restartProduct: () => inbox.restartProduct(),
    newSession: () => inbox.session(),
    asked: () => [...asked],
    kimCall: () => {
      const held = kim.current();
      if (held === null) return null;
      return {
        state: held.session.state,
        tracks: [
          ...(held.session.local?.getTracks() ?? []),
          ...(held.session.remote?.getTracks() ?? []),
        ].map((track) => `${track.kind}:${track.readyState}`),
      };
    },
    kimCalls: async (video) => {
      kimAnswers = false;
      kim.clear();
      await kim.start(YOU, video);
    },
    kimHangsUp: () => kim.current()?.session.hangUp(),
  };
}
