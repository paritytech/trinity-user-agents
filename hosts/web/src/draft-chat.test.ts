import { describe, expect, test } from "bun:test";
import type { RendererNode } from "@parity/truapi";
import type { ChatMessage, ProviderEvent } from "./chat-provider.js";
import { DraftChatSource } from "./draft-chat-source.js";
import { DraftChatProvider, type DraftSummary } from "./draft-chat.js";
import type { RenderHooks } from "./product-render.js";
import {
  ManualClock,
  MemoryStorage,
  draftChannel,
  settle,
} from "./test-support.js";

const CACHE = "draft-cache";
const STEP = 400;

/** A draft provider wired to a synthetic source over an in-memory channel. */
function connected(storage = new MemoryStorage()) {
  const clock = new ManualClock();
  const channels: ReturnType<typeof draftChannel>[] = [];
  const provider = new DraftChatProvider({
    id: "draft",
    label: "Draft",
    storage,
    cacheKey: CACHE,
    connect: () => {
      const channel = draftChannel();
      channels.push(channel);
      new DraftChatSource(channel.source, {
        now: clock.now,
        schedule: clock.schedule,
        stepMs: STEP,
      });
      return channel.host;
    },
  });
  const events: ProviderEvent[] = [];
  provider.subscribe((event) => events.push(event));
  return { provider, clock, channels, events, storage };
}

const summary = (provider: DraftChatProvider, id: string) =>
  provider.conversations().find((entry) => entry.id === id)!;

/** The statuses one message went through, as the provider's events carried it. */
const statuses = (events: ProviderEvent[], id: string) =>
  events.flatMap((event) =>
    event.type === "messages" && event.message?.id === id
      ? [event.message.status]
      : [],
  );

const textOf = (tree: RendererNode): string[] => {
  const out: string[] = [];
  const walk = (node: RendererNode) => {
    if (node.tag === "String") out.push(node.value.text);
    const children = (node.value as { children?: RendererNode[] }).children;
    for (const child of children ?? []) walk(child);
  };
  walk(tree);
  return out;
};

describe("draft worker-owned chat", () => {
  test("shows the cached list until the worker answers, then the worker's own", async () => {
    const storage = new MemoryStorage();
    const cached: DraftSummary[] = [
      {
        id: "old",
        title: "Cached",
        kind: "direct",
        preview: "",
        updatedAt: 1,
        unread: 0,
      },
    ];
    storage.setItem(CACHE, JSON.stringify(cached));
    const { provider } = connected(storage);
    expect(provider.status()).toEqual({ state: "starting" });
    expect(provider.conversations().map((entry) => entry.title)).toEqual([
      "Cached",
    ]);
    expect(provider.composer()).toEqual({
      kind: "blocked",
      reason: "The draft worker is starting.",
    });

    await settle();
    expect(provider.status()).toEqual({ state: "ready" });
    expect(provider.conversations().map((entry) => entry.title)).toEqual([
      "Ada",
      "Design group",
      "Lin",
    ]);
    expect(
      (JSON.parse(storage.getItem(CACHE)!) as DraftSummary[]).map(
        (entry) => entry.id,
      ),
    ).toEqual(["ada", "design", "lin"]);
    provider.dispose();
  });

  test("reads history a page at a time, newest page first, oldest first within it", async () => {
    const { provider } = connected();
    await settle();
    const pages: string[][] = [];
    let before: string | undefined;
    do {
      const page = await provider.timeline("ada", before);
      pages.push(page.messages.map((message) => message.id));
      before = page.before;
    } while (before !== undefined);
    expect(pages.map((page) => page.length)).toEqual([30, 30, 30, 30]);
    expect(pages[0][0]).toBe("ada-91");
    expect(pages[0].at(-1)).toBe("ada-120");
    expect(pages.reverse().flat()).toEqual(
      Array.from({ length: 120 }, (_, index) => `ada-${index + 1}`),
    );
    provider.dispose();
  });

  test("names senders by their participant id and knows which are the user's", async () => {
    const { provider, channels } = connected();
    await settle();
    const page = await provider.timeline("design");
    const senders = new Map(
      page.messages.map((message) => [message.sender.id, message.sender]),
    );
    expect([...senders.values()]).toEqual([
      { id: "ada", label: "Ada", self: false },
      { id: "grace", label: "Grace", self: false },
      { id: "me", label: "You", self: true },
    ]);
    await provider.timeline("design");
    // Names are asked for once, and never for the user's own id.
    const asked = channels[0].sent.filter(
      (message) => message.request.kind === "participants",
    );
    expect(asked.map((message) => message.request)).toEqual([
      { kind: "participants", ids: ["ada", "grace"] },
    ]);
    provider.dispose();
  });

  test("a sent message keeps its id from pending to read, and the peer answers", async () => {
    const { provider, clock, events } = connected();
    await settle();
    await provider.send("ada", "hello");
    const sent = events.find(
      (
        event,
      ): event is ProviderEvent & { type: "messages"; message: ChatMessage } =>
        event.type === "messages" && event.message?.sender.self === true,
    )!.message;
    expect(sent.status).toBe("pending");
    await clock.advance(STEP * 3);
    expect(statuses(events, sent.id)).toEqual([
      "pending",
      "sent",
      "delivered",
      "read",
    ]);
    const answer = (await provider.timeline("ada")).messages.at(-1)!;
    expect(answer.sender).toEqual({ id: "ada", label: "Ada", self: false });
    expect(answer.content).toEqual({ tag: "Text", text: 'Got it: "hello"' });
    // The user has not seen the answer, so it counts as unread until they do.
    expect(summary(provider, "ada").unread).toBe(1);
    provider.markRead("ada", answer.id);
    await settle();
    expect(summary(provider, "ada").unread).toBe(0);
    provider.dispose();
  });

  test("a failed message is sent again as the same message, not a new one", async () => {
    const { provider, clock, events } = connected();
    await settle();
    await provider.send("design", "[fail] lunch?");
    await clock.advance(STEP);
    const failed = (await provider.timeline("design")).messages.at(-1)!;
    expect(failed.status).toBe("failed");
    expect(failed.problem).toContain("dropped");
    await provider.retry("design", failed.id);
    await clock.advance(STEP * 2);
    expect(statuses(events, failed.id)).toEqual([
      "pending",
      "failed",
      "pending",
      "sent",
      "delivered",
    ]);
    const mine = (await provider.timeline("design")).messages.filter(
      (message) =>
        message.sender.self &&
        message.content.tag === "Text" &&
        message.content.text === "[fail] lunch?",
    );
    expect(mine.map((message) => message.id)).toEqual([failed.id]);
    await expect(provider.retry("design", failed.id)).rejects.toThrow(
      "Only a failed message can be sent again.",
    );
    provider.dispose();
  });

  test("opening a conversation is not reading it; only markRead clears unread", async () => {
    const { provider } = connected();
    await settle();
    expect(summary(provider, "lin").unread).toBe(3);
    const page = await provider.timeline("lin");
    await settle();
    expect(summary(provider, "lin").unread).toBe(3);
    provider.markRead("lin", page.messages[4].id);
    await settle();
    expect(summary(provider, "lin").unread).toBe(1);
    // Seen through an older message changes nothing.
    provider.markRead("lin", page.messages[0].id);
    await settle();
    expect(summary(provider, "lin").unread).toBe(1);
    provider.dispose();
  });

  test("draws a custom message as the worker's tree, and carries its button back", async () => {
    const { provider, channels } = connected();
    await settle();
    const poll = (await provider.timeline("design")).messages.at(-1)!;
    expect(poll.content.tag).toBe("Custom");
    const drawn: { tree: RendererNode; hooks: RenderHooks }[] = [];
    const stop = provider.renderCustom("design", poll, {
      draw: (tree, hooks) => drawn.push({ tree, hooks }),
      fail: (reason) => {
        throw new Error(reason);
      },
    });
    await settle();
    expect(textOf(drawn.at(-1)!.tree)).toEqual(["Lunch at noon?", "0 votes"]);
    drawn.at(-1)!.hooks.action("vote", new Uint8Array());
    await settle();
    expect(textOf(drawn.at(-1)!.tree)).toEqual(["Lunch at noon?", "1 vote"]);
    stop();
    await settle();
    expect(channels[0].sent.at(-1)!.request.kind).toBe("stopRender");
    provider.dispose();
  });

  test("a restart refuses what the old worker owed and moves the epoch", async () => {
    const { provider, channels } = connected();
    await settle();
    const late = provider.timeline("ada");
    provider.restart();
    expect(provider.status()).toEqual({ state: "starting" });
    expect(provider.epoch()).toBe(1);
    expect(channels[0].closed()).toBe(true);
    await expect(late).rejects.toThrow("The draft worker restarted.");
    await settle();
    expect(provider.status()).toEqual({ state: "ready" });
    expect((await provider.timeline("ada")).messages.length).toBe(30);
    provider.dispose();
    expect(channels[1].closed()).toBe(true);
    await expect(provider.timeline("ada")).rejects.toThrow(
      "The draft worker is not running.",
    );
  });
});
