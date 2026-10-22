import { describe, expect, test } from "bun:test";
import type { ProductContext } from "@parity/truapi-host";
import { ChatDiagnostics } from "./chat/diagnostics.js";
import {
  ChatInbox,
  DRAFT_PROVIDER_ID,
  type InboxChange,
} from "./chat-inbox.js";
import { DraftChatSource } from "./draft-chat-source.js";
import { draftCacheKey, draftEnabledKey } from "./draft-chat.js";
import type { ChatSession } from "./nova-chat-provider.js";
import { ProductChat, productChatKey } from "./product-chat.js";
import {
  ManualClock,
  MemoryStorage,
  draftChannel,
  settle,
} from "./test-support.js";

const worker = (productId: string): ProductContext => ({
  productId,
  executionKind: "Worker",
});

/** A session whose Nova chat stays off, so only product and draft providers run. */
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
  const channels: ReturnType<typeof draftChannel>[] = [];
  const clock = new ManualClock();
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
    draftWorker: () => {
      const channel = draftChannel();
      channels.push(channel);
      new DraftChatSource(channel.source, {
        now: clock.now,
        schedule: clock.schedule,
      });
      return channel.host;
    },
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
  return { inbox, storage, chat, channels, changes, signIn, setWorkers };
}

const ids = (inbox: ChatInbox) =>
  inbox.providers().map((provider) => provider.id);

describe("chat inbox", () => {
  test("has one provider per listed worker or product with rooms, and the draft only when turned on", async () => {
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
    inbox.setDraftEnabled(true);
    expect(ids(inbox)).toContain(DRAFT_PROVIDER_ID);
    setWorkers([]);
    expect(ids(inbox)).toEqual(["product:quiz.paseo", DRAFT_PROVIDER_ID]);
  });

  test("a new session ends every provider of the old one, and nothing it owed arrives", async () => {
    const { inbox, storage, channels, changes, signIn } = setup();
    signIn("w1");
    inbox.setDraftEnabled(true);
    await settle();
    const first = inbox
      .providers()
      .find((provider) => provider.id === DRAFT_PROVIDER_ID)!;
    const owed = first.timeline("ada");

    signIn("w2");
    expect(channels[0].closed()).toBe(true);
    await expect(owed).rejects.toThrow("The draft chat stopped.");
    expect(changes.at(-1)).toEqual({ type: "session" });
    // The draft is per wallet: off for the second until it is turned on there.
    expect(inbox.draftEnabled()).toBe(false);
    expect(ids(inbox)).toEqual([]);

    signIn("w1");
    expect(inbox.draftEnabled()).toBe(true);
    await settle();
    const again = inbox
      .providers()
      .find((provider) => provider.id === DRAFT_PROVIDER_ID)!;
    expect(again).not.toBe(first);
    expect(again.status()).toEqual({ state: "ready" });
    expect(storage.getItem(draftEnabledKey("paseo", "w1"))).toBe("on");
    expect(storage.getItem(draftCacheKey("paseo", "w2"))).toBeNull();

    inbox.setSession(null);
    expect(channels.every((channel) => channel.closed())).toBe(true);
    expect(inbox.signedIn()).toBe(false);
  });
});
