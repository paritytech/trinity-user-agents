import { describe, expect, test } from "bun:test";
import type { ChatMessage, ChatParticipantEntry, ProductChatParticipantsQueryItem } from "@parity/truapi";
import {
  answerMessageActions,
  answerMessages,
  answerParticipantActions,
  answerParticipants,
  answerReactions,
  type QueryContext,
} from "../../worker/queries";
import { initialState } from "../../worker/state";
import { SourceStore } from "../../worker/store";

function context(): QueryContext {
  return { rooms: initialState().rooms, store: new SourceStore(), product: "sample.dot" };
}

function okValue<T>(answer: { ok: T } | { error: unknown }): T {
  if (!("ok" in answer)) throw new Error(`expected an answer, got ${JSON.stringify(answer)}`);
  return answer.ok;
}

function pageOf(item: ProductChatParticipantsQueryItem): { participants: ChatParticipantEntry[]; next?: string } {
  if (item.tag !== "Page") throw new Error(`expected a page, got ${item.tag}`);
  return item.value;
}

const message: ChatMessage = {
  messageId: "host-p-0001",
  origin: { participantId: "product" },
  timestamp: 1n,
  body: { tag: "Custom", value: { messageType: "x", payload: "0x" } },
  status: "Sent",
  reactions: [],
};

describe("participants_query", () => {
  test("counts p1 to p4 and the guests", () => {
    const ctx = context();
    expect(okValue(answerParticipants(ctx, { roomId: "source", query: { tag: "Count" } }))).toEqual({
      tag: "Count",
      value: { count: 4 },
    });
    ctx.rooms.source.guests = 10_000;
    expect(okValue(answerParticipants(ctx, { roomId: "source", query: { tag: "Count" } }))).toEqual({
      tag: "Count",
      value: { count: 10_004 },
    });
  });

  test("pages 10,004 participants in id order, at most 100 a page, each once", () => {
    const ctx = context();
    ctx.rooms.source.guests = 10_000;
    const ids: string[] = [];
    let cursor: string | undefined;
    let pages = 0;
    do {
      const query = { tag: "Page" as const, value: cursor === undefined ? { pageSize: 200 } : { cursor, pageSize: 200 } };
      const page = pageOf(okValue(answerParticipants(ctx, { roomId: "source", query })));
      expect(page.participants.length).toBeLessThanOrEqual(100);
      ids.push(...page.participants.map((p) => p.participantId));
      cursor = page.next;
      pages++;
    } while (cursor !== undefined);
    expect(pages).toBe(101);
    expect(ids.length).toBe(10_004);
    expect(new Set(ids).size).toBe(10_004);
    expect(ids.slice(0, 6)).toEqual(["p1", "p2", "p3", "p4", "p5", "p6"]);
    expect(ids.at(-1)).toBe("p10004");
  });

  test("gives every participant kind, and guests as Custom", () => {
    const ctx = context();
    ctx.rooms.host.guests = 1;
    const page = pageOf(okValue(answerParticipants(ctx, { roomId: "host", query: { tag: "Page", value: { pageSize: 10 } } })));
    expect(page.participants.map((p) => p.participant.tag)).toEqual(["Person", "LitePerson", "Contact", "Product", "Custom"]);
    expect(page.participants[4]).toEqual({ participantId: "p5", participant: { tag: "Custom", value: { nickname: "Guest 5" } } });
    expect(page.next).toBeUndefined();
  });

  test("resolves known ids and leaves out the rest", () => {
    const ctx = context();
    ctx.rooms.source.guests = 3;
    const item = okValue(
      answerParticipants(ctx, { roomId: "source", query: { tag: "ByIds", value: { participantIds: ["p2", "p7", "p8", "user", "p0"] } } }),
    );
    expect(item.tag).toBe("Resolved");
    if (item.tag === "Resolved") expect(item.value.participants.map((p) => p.participantId)).toEqual(["p2", "p7"]);
  });

  test("answers UnknownRoom for a room of no product", () => {
    expect(answerParticipants(context(), { roomId: "other", query: { tag: "Count" } })).toEqual({ error: { tag: "UnknownRoom" } });
  });
});

describe("message_actions_query", () => {
  test("answers each menu mode", () => {
    const ctx = context();
    const ask = () => okValue(answerMessageActions(ctx, { roomId: "host", message })).menu;
    expect(ask()).toEqual({ tag: "ExtendBuiltIn", value: { actions: [], hidden: [] } });
    ctx.rooms.host.menu = "custom";
    const custom = ask();
    expect(custom.tag).toBe("Custom");
    expect(custom.value.actions.map((a) => [a.actionId, a.destructive])).toEqual([
      ["act-pin", false],
      ["act-report", true],
    ]);
    ctx.rooms.host.menu = "extend";
    expect(ask()).toEqual({ tag: "ExtendBuiltIn", value: { actions: [custom.value.actions[0]], hidden: [] } });
    ctx.rooms.host.menu = "hide-edit";
    expect(ask()).toEqual({ tag: "ExtendBuiltIn", value: { actions: [custom.value.actions[0]], hidden: ["Edit"] } });
  });

  test("answers the deeplink menu for a /deeplink message", () => {
    const ctx = context();
    ctx.rooms.host.deeplinkMessages.push("host-p-0001");
    expect(okValue(answerMessageActions(ctx, { roomId: "host", message })).menu).toEqual({
      tag: "Custom",
      value: {
        actions: [
          {
            actionId: "act-open-app",
            label: "Open the App",
            destructive: false,
            handler: { tag: "Deeplink", value: { url: "polkadot://sample.dot/?from=deeplink" } },
          },
        ],
      },
    });
  });
});

describe("reactions_query and the other queries", () => {
  test("reactions: on lists them, na and off answer NotAvailable", () => {
    const ctx = context();
    ctx.rooms.host.reactions = "on";
    ctx.rooms.host.reactionBook["host-p-0001"] = [{ participantId: "p1", emoji: "👍" }];
    expect(okValue(answerReactions(ctx, { roomId: "host", messageId: "host-p-0001" }))).toEqual({
      reactions: [{ participantId: "p1", emoji: "👍" }],
    });
    ctx.rooms.host.reactions = "na";
    expect(answerReactions(ctx, { roomId: "host", messageId: "host-p-0001" })).toEqual({ error: { tag: "NotAvailable" } });
  });

  test("messages_query serves source only", () => {
    const ctx = context();
    expect(answerMessages(ctx, { roomId: "host", anchor: { tag: "Before", value: {} }, pageSize: 10 })).toEqual({
      error: { tag: "UnknownRoom" },
    });
    expect(answerMessages(ctx, { roomId: "source", anchor: { tag: "Before", value: { cursor: "x" } }, pageSize: 10 })).toEqual({
      error: { tag: "Unknown", value: { reason: "bad cursor x" } },
    });
  });

  test("participant_actions_query adds Mention", () => {
    expect(okValue(answerParticipantActions(context(), { roomId: "source" })).menu).toEqual({
      tag: "ExtendBuiltIn",
      value: { actions: [{ actionId: "act-mention", label: "Mention", destructive: false, handler: { tag: "Callback" } }] },
    });
  });
});
