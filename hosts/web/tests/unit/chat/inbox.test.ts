import { describe, expect, test } from "bun:test";
import type { ProductContext } from "@parity/truapi-host";
import { ChatDiagnostics } from "../../../tools/inspector/diagnostics.js";
import { ChatInbox, type InboxChange } from "../../../src/chat/inbox.js";
import { messagePortBridge } from "../../../src/chat/draft/contract.js";
import { ChatV2Host } from "../../../src/chat/draft/host.js";
import { chatV2Over } from "../../../src/chat/draft/product.js";
import type { ChatSession } from "../../../src/chat/nova-provider.js";
import { ProductChat, productChatKey } from "../../../src/chat/product-chat.js";
import { MemoryStorage } from "../../support/test-support.js";

const worker = (productId: string): ProductContext => ({
  productId,
  executionKind: "Worker",
});

/** A session whose Nova chat stays off, so only product providers run. */
const session = (walletId: string): ChatSession =>
  ({
    walletId,
    entropy: new Uint8Array(16).fill(walletId.length),
    network: "paseo",
    networkSuffix: "paseo",
    identityAccountId: undefined,
    chatPublicKey: undefined,
    username: undefined,
  }) as unknown as ChatSession;

function setup() {
  const storage = new MemoryStorage();
  const chat = new ProductChat(storage, () => 1);
  const chatV2 = new ChatV2Host({ media: () => null, log: () => {} });
  const workers = new Set<() => void>();
  let names: string[] = [];
  const inbox = new ChatInbox({
    storage,
    diagnostics: new ChatDiagnostics(),
    rooms: {
      chat,
      channel: () => null,
      workerState: () => null,
      onWorkersChange: (listener) => {
        workers.add(listener);
        return () => workers.delete(listener);
      },
      archiveImage: async () => null,
      gateway: () => "https://gateway.invalid",
      log: () => {},
    },
    workerNames: () => names,
    log: () => {},
    chatV2,
  });
  const changes: InboxChange[] = [];
  inbox.onChange((change) => changes.push(change));
  const signIn = (walletId: string) => {
    chat.setScope(productChatKey("paseo", walletId));
    inbox.setSession(session(walletId));
  };
  const setWorkers = (next: string[]) => {
    names = next;
    for (const listener of workers) listener();
  };
  return { inbox, chat, chatV2, changes, signIn, setWorkers };
}

const ids = (inbox: ChatInbox) =>
  inbox.providers().map((provider) => provider.id);

describe("chat inbox", () => {
  test("has one room provider per listed worker or product with rooms, and one chat product provider per listed worker", async () => {
    const { inbox, chat, signIn, setWorkers } = setup();
    signIn("w1");
    expect(inbox.problem()).toContain("did not report");
    expect(ids(inbox)).toEqual([]);
    setWorkers(["game.paseo"]);
    await chat.platform.createChatRoom(worker("quiz.paseo"), {
      roomId: "r",
      name: "Quiz",
      icon: "",
    });
    expect(ids(inbox)).toEqual([
      "product:game.paseo",
      "product:quiz.paseo",
      "chat-v2:game.paseo",
    ]);
    setWorkers([]);
    expect(ids(inbox)).toEqual(["product:quiz.paseo"]);
  });

  test("a new session ends every provider of the old one", async () => {
    const { inbox, chatV2, changes, signIn, setWorkers } = setup();
    setWorkers(["chat.paseo"]);
    signIn("w1");
    const first = inbox
      .providers()
      .find((provider) => provider.id === "chat-v2:chat.paseo")!;
    const channel = new MessageChannel();
    chatV2.connect("chat.paseo", messagePortBridge(channel.port2));
    const product = chatV2Over(messagePortBridge(channel.port1));
    await product.createRoom({
      roomId: "ada",
      metadata: { name: "Ada" },
      config: {
        productSource: true,
        reactionDetails: false,
        participants: false,
        messageActions: false,
        reportSeen: false,
      },
    });
    const heard: unknown[] = [];
    first.subscribe((event) => heard.push(event));
    expect(first.conversations().map((room) => room.id)).toEqual(["ada"]);

    signIn("w2");
    expect(changes.at(-1)).toEqual({ type: "session" });
    const second = inbox
      .providers()
      .find((provider) => provider.id === "chat-v2:chat.paseo")!;
    expect(second).not.toBe(first);
    // The old provider hears nothing more.
    await product.setRoomMetadata({ roomId: "ada", metadata: { name: "x" } });
    expect(heard).toEqual([]);

    inbox.setSession(null);
    expect(ids(inbox)).toEqual([]);
    expect(inbox.signedIn()).toBe(false);
  });
});
