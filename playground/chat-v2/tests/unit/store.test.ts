import { describe, expect, test } from "bun:test";
import type { ChatMessage, ChatPageAnchor } from "@parity/truapi";
import { historyMessage } from "../../worker/fixtures";
import { MAX_PAGE_SIZE, SourceStore, StoreError, summarize } from "../../worker/store";

function live(id: string, timestamp: bigint): ChatMessage {
  return {
    messageId: id,
    origin: { participantId: "product" },
    timestamp,
    body: { tag: "Text", value: { text: id } },
    status: "Sent",
    reactions: [],
  };
}

/** Walk every page in one direction and give the ids, oldest first. */
function walk(store: SourceStore, direction: "Before" | "After", size: number): { ids: string[]; cursors: string[] } {
  const pages: string[][] = [];
  const cursors: string[] = [];
  let cursor: string | undefined;
  for (;;) {
    const anchor: ChatPageAnchor = { tag: direction, value: cursor === undefined ? {} : { cursor } };
    const page = store.page(anchor, size);
    pages.push(page.messages.map((m) => m.messageId));
    if (page.next === undefined) break;
    cursors.push(page.next);
    cursor = page.next;
  }
  const ordered = direction === "Before" ? pages.reverse() : pages;
  return { ids: ordered.flat(), cursors };
}

describe("generated history", () => {
  test("every fifth message is by p1 to p4 in turn, the rest by the product", () => {
    const authors = Array.from({ length: 20 }, (_, i) => historyMessage(i + 1, 20).message.origin.participantId);
    expect(authors.filter((a) => a !== "product")).toEqual(["p1", "p2", "p3", "p4"]);
    expect(authors[4]).toBe("p1");
    expect(authors[19]).toBe("p4");
    expect(authors[0]).toBe("product");
  });

  test("every 25th replies to the one 10 before and every 50th has a p1 reaction", () => {
    expect(historyMessage(25, 100).message.replyTo).toBe("source-h-00015");
    expect(historyMessage(24, 100).message.replyTo).toBeUndefined();
    expect(historyMessage(50, 100).reactions).toEqual([{ participantId: "p1", emoji: "👍" }]);
    expect(historyMessage(49, 100).reactions).toEqual([]);
    expect(historyMessage(17, 100).message.body).toEqual({ tag: "Text", value: { text: "History message 00017" } });
  });

  test("is the same on every call", () => {
    expect(historyMessage(50, 1000)).toEqual(historyMessage(50, 1000));
  });
});

describe("SourceStore paging", () => {
  test("Before pages cover the room once, newest page first", () => {
    const store = new SourceStore();
    store.replaceHistory(250);
    store.add(live("source-p-0001", 1n));
    const { ids, cursors } = walk(store, "Before", 100);
    expect(ids.length).toBe(251);
    expect(new Set(ids).size).toBe(251);
    expect(ids[0]).toBe("source-h-00001");
    expect(ids.at(-1)).toBe("source-p-0001");
    expect(cursors).toEqual(["151", "51"]);
  });

  test("the first Before page is the newest messages, oldest first", () => {
    const store = new SourceStore();
    store.replaceHistory(30);
    const page = store.page({ tag: "Before", value: {} }, 10);
    expect(page.messages.map((m) => m.messageId)).toEqual(
      Array.from({ length: 10 }, (_, i) => `source-h-000${21 + i}`),
    );
    expect(page.next).toBe("20");
  });

  test("After pages cover the room once, oldest first", () => {
    const store = new SourceStore();
    store.replaceHistory(205);
    const { ids, cursors } = walk(store, "After", 100);
    expect(ids.length).toBe(205);
    expect(cursors).toEqual(["100", "200"]);
  });

  test("clamps the page size to the RFC bound", () => {
    const store = new SourceStore();
    store.replaceHistory(300);
    expect(store.page({ tag: "Before", value: {} }, 500).messages.length).toBe(MAX_PAGE_SIZE);
  });

  test("an empty room has one empty page", () => {
    expect(new SourceStore().page({ tag: "Before", value: {} }, 50)).toEqual({ messages: [] });
  });

  test("skips deleted messages and keeps the cursors stable", () => {
    const store = new SourceStore();
    store.replaceHistory(20);
    const before = store.page({ tag: "Before", value: {} }, 10).next;
    store.delete("source-h-00020");
    store.delete("source-h-00005");
    const { ids } = walk(store, "Before", 10);
    expect(ids.length).toBe(18);
    expect(ids).not.toContain("source-h-00020");
    expect(store.page({ tag: "Before", value: {} }, 10).next).toBe(before);
  });

  test("refuses a bad cursor", () => {
    const store = new SourceStore();
    store.replaceHistory(5);
    expect(() => store.page({ tag: "Before", value: { cursor: "x" } }, 5)).toThrow(StoreError);
    expect(() => store.page({ tag: "After", value: { cursor: "6" } }, 5)).toThrow(StoreError);
  });
});

describe("SourceStore changes", () => {
  test("edits, reactions and status of a history message survive a snapshot", () => {
    const store = new SourceStore();
    store.replaceHistory(60);
    store.edit("source-h-00050", { tag: "Text", value: { text: "edited" } }, 5n);
    store.react("source-h-00050", "user", "👍", true);
    store.react("source-h-00050", "p2", "🎉", true);
    store.setStatus("source-h-00050", "Read");
    const copy = new SourceStore(JSON.parse(JSON.stringify(store.snapshot(), (_k, v) => (typeof v === "bigint" ? Number(v) : v))));
    const message = copy.get("source-h-00050");
    expect(message?.body).toEqual({ tag: "Text", value: { text: "edited" } });
    expect(message?.status).toBe("Read");
    expect(message?.reactions).toEqual([
      { emoji: "👍", count: 2, reactedByUser: true },
      { emoji: "🎉", count: 1, reactedByUser: false },
    ]);
    expect(store.reactionDetails("source-h-00050")).toEqual([
      { participantId: "p1", emoji: "👍" },
      { participantId: "user", emoji: "👍" },
      { participantId: "p2", emoji: "🎉" },
    ]);
  });

  test("a participant reacts once per emoji", () => {
    const store = new SourceStore();
    store.add(live("a", 1n));
    store.react("a", "p1", "👍", true);
    store.react("a", "p1", "👍", true);
    expect(store.get("a")?.reactions).toEqual([{ emoji: "👍", count: 1, reactedByUser: false }]);
    store.react("a", "p1", "👍", false);
    expect(store.get("a")?.reactions).toEqual([]);
  });

  test("refuses unknown and duplicate messages", () => {
    const store = new SourceStore();
    store.add(live("a", 1n));
    expect(() => store.add(live("a", 2n))).toThrow(StoreError);
    expect(() => store.edit("b", { tag: "Text", value: { text: "x" } }, 1n)).toThrow(StoreError);
    store.delete("a");
    expect(store.has("a")).toBe(false);
    expect(() => store.delete("a")).toThrow(StoreError);
  });

  test("replaceHistory clears the live messages", () => {
    const store = new SourceStore();
    store.add(live("a", 1n));
    store.replaceHistory(0);
    expect(store.messages()).toEqual([]);
  });

  test("summarize counts each emoji in order of first reaction", () => {
    expect(
      summarize([
        { participantId: "p1", emoji: "🎉" },
        { participantId: "user", emoji: "👍" },
        { participantId: "p2", emoji: "🎉" },
      ]),
    ).toEqual([
      { emoji: "🎉", count: 2, reactedByUser: false },
      { emoji: "👍", count: 1, reactedByUser: true },
    ]);
  });
});

describe("search", () => {
  test("pages the matches newest first with the slot cursor, case ignored", () => {
    const store = new SourceStore();
    store.replaceHistory(30);
    // History texts are "History message 000NN"; every message matches "history".
    const first = store.search("HISTORY message 0001", undefined, 3);
    expect(first.messages.map((m) => m.messageId)).toEqual(["source-h-00019", "source-h-00018", "source-h-00017"]);
    expect(first.next).toBeDefined();
    const second = store.search("history message 0001", first.next, 10);
    expect(second.messages.map((m) => m.messageId)).toEqual([
      "source-h-00016",
      "source-h-00015",
      "source-h-00014",
      "source-h-00013",
      "source-h-00012",
      "source-h-00011",
      "source-h-00010",
    ]);
    expect(second.next).toBeUndefined();
    expect(store.search("nothing like this", undefined, 10)).toEqual({ messages: [] });
  });
});
