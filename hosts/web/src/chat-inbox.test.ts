import { describe, expect, test } from "bun:test";
import type { ProductContext } from "@parity/truapi-host";
import { ChatDiagnostics } from "./chat/diagnostics.js";
import { ChatInbox, type InboxChange } from "./chat-inbox.js";
import type { ChatSession } from "./nova-chat-provider.js";
import { ProductChat, productChatKey } from "./product-chat.js";
import { MemoryStorage } from "./test-support.js";

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
  return { inbox, chat, changes, signIn, setWorkers };
}

const ids = (inbox: ChatInbox) =>
  inbox.providers().map((provider) => provider.id);

describe("chat inbox", () => {
  test("has one provider per listed worker or product with rooms", async () => {
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
    expect(ids(inbox)).toEqual(["product:game.paseo", "product:quiz.paseo"]);
    setWorkers([]);
    expect(ids(inbox)).toEqual(["product:quiz.paseo"]);
  });

  test("a new session ends every provider of the old one", async () => {
    const { inbox, chat, changes, signIn } = setup();
    signIn("w1");
    await chat.platform.createChatRoom(worker("quiz.paseo"), {
      roomId: "r",
      name: "Quiz",
      icon: "",
    });
    const [first] = inbox.providers();
    const events: unknown[] = [];
    first.subscribe((event) => events.push(event));

    signIn("w2");
    expect(changes.at(-1)).toEqual({ type: "session" });
    expect(ids(inbox)).toEqual([]);
    // The old provider was disposed, so a change to the rooms reaches nobody.
    await chat.platform.createChatRoom(worker("quiz.paseo"), {
      roomId: "s",
      name: "Quiz",
      icon: "",
    });
    expect(events).toEqual([]);
    expect(inbox.providers()[0]).not.toBe(first);

    inbox.setSession(null);
    expect(inbox.signedIn()).toBe(false);
    expect(ids(inbox)).toEqual([]);
  });
});
