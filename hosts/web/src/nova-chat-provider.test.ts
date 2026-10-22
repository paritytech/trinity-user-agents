import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import { ChatDiagnostics } from "./chat/diagnostics.js";
import type { ChatSnapshot } from "./chat/engine.js";
import type { Contact, StoredMessage } from "./chat/store.js";
import type { ChatProvider, ProviderEvent } from "./chat-provider.js";
import {
  NovaChatProvider,
  startNovaChat,
  type ChatSession,
  type NovaEngine,
} from "./nova-chat-provider.js";
import { MemoryStorage } from "./test-support.js";

const ME = `0x${"11".repeat(32)}` as HexString;
const ADA = `0x${"aa".repeat(32)}` as HexString;
const BOB = `0x${"bb".repeat(32)}` as HexString;

const message = (
  id: string,
  direction: "in" | "out",
  at: number,
  extra: Partial<StoredMessage> = {},
): StoredMessage => ({
  id,
  direction,
  kind: "text",
  text: `text ${id}`,
  at,
  status: direction === "in" ? "received" : "acknowledged",
  ...extra,
});

const contact = (
  account: HexString,
  name: string | null,
  messages: StoredMessage[],
  state: Contact["state"] = "active",
): Contact => ({
  account,
  chatKey: `0x${"cc".repeat(32)}`,
  name,
  state,
  messages,
  outbound: null,
  expiry: { request: null, response: null },
  acknowledged: null,
});

/** An engine that holds a snapshot the test edits, and records what it is asked. */
function fakeEngine(snapshot: ChatSnapshot) {
  const listeners = new Set<() => void>();
  const calls: string[] = [];
  const engine: NovaEngine = {
    snapshot: () => snapshot,
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    start: async () => {},
    stop: () => void calls.push("stop"),
    accept: async (id) => {
      calls.push(`accept ${id}`);
      snapshot.requests = [];
      snapshot.contacts.push(contact(BOB, "bob.01", []));
    },
    decline: (id) => void calls.push(`decline ${id}`),
    startChat: async (target, welcome) => {
      calls.push(`start ${target} ${welcome}`);
      snapshot.contacts.push(
        contact(
          BOB,
          "bob.01",
          [
            message("r1", "out", 9, {
              kind: "request",
              text: welcome,
              status: "sent",
            }),
          ],
          "requested",
        ),
      );
    },
    send: async (account, text) => void calls.push(`send ${account} ${text}`),
    retry: async (account) => void calls.push(`retry ${account}`),
  };
  const change = () => {
    for (const listener of listeners) listener();
  };
  return { engine, calls, change };
}

function snapshotWith(contacts: Contact[]): ChatSnapshot {
  return {
    status: { state: "listening" },
    account: ME,
    chatKey: `0x${"dd".repeat(32)}`,
    name: "me.01",
    chainKey: "matches",
    canSend: true,
    requests: [],
    contacts,
    skipped: [],
  };
}

describe("Nova chat provider", () => {
  test("a contact is a conversation, previewed as the screen showed it", () => {
    const snapshot = snapshotWith([
      contact(ADA, "ada.01", [message("m1", "in", 5), message("m2", "out", 7)]),
    ]);
    const provider = new NovaChatProvider(fakeEngine(snapshot).engine);
    expect(provider.conversations()).toEqual([
      {
        id: ADA,
        title: "ada.01",
        subtitle: "0xaaaaaa…aaaaaa",
        icon: null,
        seed: ADA,
        preview: "You: text m2",
        updatedAt: 7,
      },
    ]);
    expect(provider.status()).toEqual({ state: "ready" });
    // Nova keeps no read state, so the screen reports nothing to it.
    expect((provider as ChatProvider).markRead).toBeUndefined();
  });

  test("reads a contact's messages in pages, the user's as self, with Nova's statuses", async () => {
    const messages = Array.from({ length: 45 }, (_, index) =>
      message(`m${index}`, index % 2 === 0 ? "in" : "out", index),
    );
    messages[44] = message("m44", "out", 44, {
      status: "failed",
      problem: "refused",
    });
    const provider = new NovaChatProvider(
      fakeEngine(snapshotWith([contact(ADA, null, messages)])).engine,
    );
    const newest = await provider.timeline(ADA);
    expect(newest.messages.length).toBe(30);
    expect(newest.messages[0].id).toBe("m15");
    const older = await provider.timeline(ADA, newest.before);
    expect(older.messages.map((entry) => entry.id)).toEqual(
      messages.slice(0, 15).map((entry) => entry.id),
    );
    expect(older.before).toBeUndefined();
    expect(newest.messages.at(-2)).toMatchObject({
      sender: { id: ME, label: "You", self: true },
      status: "delivered",
    });
    expect(newest.messages.at(-1)).toMatchObject({
      status: "failed",
      problem: "refused",
    });
    expect(older.messages[0].sender).toEqual({
      id: ADA,
      label: "0xaaaaaa…aaaaaa",
      self: false,
    });
  });

  test("keeps Nova's own steps: requests, starting by username, and a waiting composer", async () => {
    const snapshot = snapshotWith([]);
    snapshot.requests = [
      {
        id: "q1",
        peer: BOB,
        peerChatKey: null,
        name: "bob.01",
        welcome: null,
        at: 3,
        version: 2,
        problem: null,
      },
    ];
    const { engine, calls } = fakeEngine(snapshot);
    const provider = new NovaChatProvider(engine);
    expect(provider.requests.list()).toEqual([
      {
        id: "q1",
        title: "bob.01",
        seed: BOB,
        preview: "Wants to chat",
        at: 3,
        problem: null,
      },
    ]);
    expect(await provider.requests.accept("q1")).toBe(BOB);
    provider.requests.decline("q2");

    const other = snapshotWith([]);
    const started = fakeEngine(other);
    const starter = new NovaChatProvider(started.engine);
    expect(starter.start.blocked()).toBeNull();
    expect(await starter.start.start("bob.01", "hi")).toBe(BOB);
    expect(started.calls).toEqual(["start bob.01 hi"]);
    expect(starter.composer(BOB)).toEqual({
      kind: "blocked",
      reason: "Wait for bob.01 to accept your message request.",
    });
    const [request] = (await starter.timeline(BOB)).messages;
    expect(request.content).toEqual({
      tag: "Text",
      text: "hi",
      label: "Your chat request",
    });
    other.canSend = false;
    expect(starter.start.blocked()).toBe("No statement allowance.");
    expect(calls).toEqual(["accept q1", "decline q2"]);
  });

  test("tells the screen which contact changed, and sends and retries through the engine", async () => {
    const snapshot = snapshotWith([
      contact(ADA, "ada.01", [message("m1", "in", 1)]),
      contact(BOB, "bob.01", [message("n1", "in", 2)]),
    ]);
    const { engine, calls, change } = fakeEngine(snapshot);
    const provider = new NovaChatProvider(engine);
    const events: ProviderEvent[] = [];
    provider.subscribe((event) => events.push(event));
    snapshot.contacts[0].messages.push(message("m2", "in", 3));
    change();
    expect(events.filter((event) => event.type === "messages")).toEqual([
      { type: "messages", conversationId: ADA },
    ]);
    await provider.send(ADA, "hello");
    await provider.retry(ADA);
    provider.dispose();
    expect(calls).toEqual([`send ${ADA} hello`, `retry ${ADA}`, "stop"]);
  });

  test("stays off, with the reason, when the core reports no identity for the session", () => {
    const session = {
      walletId: "w1",
      entropy: crypto.getRandomValues(new Uint8Array(16)),
      network: "paseo",
      networkSuffix: "paseo",
      identityAccountId: undefined,
      chatPublicKey: undefined,
      username: undefined,
    } as unknown as ChatSession;
    expect(
      startNovaChat(session, {
        storage: new MemoryStorage(),
        diagnostics: new ChatDiagnostics(),
      }),
    ).toBe(
      "The core did not report this session's identity account and chat key, so chat cannot check its keys.",
    );
  });
});
