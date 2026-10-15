import { describe, expect, it } from "bun:test";

import type { ChatQueryName, WasmChatQueryHandle } from "./chat-queries.js";
import type { WorkerProductRuntime } from "./wasm-module.js";
import {
  handleChatQueryStart,
  handleStageChatAttachment,
  handleWakeProduct,
  stopChatQueriesForCore,
  stopChatQuery,
  type ChatQueries,
} from "./worker-chat-queries.js";
import type { WorkerToMain } from "./worker-protocol.js";

interface Callbacks {
  answer: (item: Uint8Array) => void;
  complete: () => void;
  refuse: (error: Uint8Array) => void;
}

/** A core stub that keeps the callbacks of each query it starts. */
function fakeCore(log: string[], started: Callbacks[]): WorkerProductRuntime {
  const query =
    (name: ChatQueryName) =>
    (
      request: Uint8Array,
      onAnswer: (item: Uint8Array) => void,
      onComplete: () => void,
      onError: (error: Uint8Array) => void,
    ): WasmChatQueryHandle => {
      log.push(`${name}:${request.join(",")}`);
      started.push({ answer: onAnswer, complete: onComplete, refuse: onError });
      return {
        cancel: () => log.push("cancel"),
        free: () => log.push("free"),
      };
    };
  return {
    receiveFrame: async () => {},
    notifyContactsChanged: () => {},
    dispose: () => {},
    free: () => {},
    publishChatAction: () => {},
    publishChatEvent: () => {},
    publishChatRoomEvent: () => {},
    wakeProduct: (roomId, watchId) => {
      log.push(`wake:${roomId ?? "-"}:${watchId ?? "-"}`);
      return watchId === "spent" ? undefined : 30_000;
    },
    stageChatAttachment: async () => {
      throw new Error("this core does not stage chat attachments yet");
    },
    publishRendererAction: () => {},
    render: () => ({ cancel: () => {}, free: () => {} }),
    chatMessagesQuery: query("chatMessagesQuery"),
    chatUserAction: query("chatUserAction"),
    chatReactionsQuery: query("chatReactionsQuery"),
    chatParticipantsQuery: query("chatParticipantsQuery"),
    chatMessageActionsQuery: query("chatMessageActionsQuery"),
    chatParticipantActionsQuery: query("chatParticipantActionsQuery"),
    chatSearchQuery: query("chatSearchQuery"),
  };
}

describe("worker chat queries", () => {
  it("sends the answer of a query and releases its handle", () => {
    const messages: WorkerToMain[] = [];
    const log: string[] = [];
    const started: Callbacks[] = [];
    const queries: ChatQueries = new Map();
    const core = fakeCore(log, started);

    handleChatQueryStart(
      core,
      (msg) => messages.push(msg),
      queries,
      1,
      7,
      "chatMessagesQuery",
      new Uint8Array([1, 2]),
    );
    expect(queries.has(7)).toBe(true);
    started[0]!.answer(new Uint8Array([9]));

    expect(messages).toEqual([
      { kind: "chatQueryAnswer", queryId: 7, item: new Uint8Array([9]) },
    ]);
    expect(queries.has(7)).toBe(false);
    expect(log).toEqual(["chatMessagesQuery:1,2", "cancel", "free"]);
  });

  it("sends the refusal and the end of a query", () => {
    const messages: WorkerToMain[] = [];
    const started: Callbacks[] = [];
    const queries: ChatQueries = new Map();
    const core = fakeCore([], started);
    const post = (msg: WorkerToMain) => messages.push(msg);

    handleChatQueryStart(
      core,
      post,
      queries,
      1,
      1,
      "chatUserAction",
      new Uint8Array(),
    );
    handleChatQueryStart(
      core,
      post,
      queries,
      1,
      2,
      "chatReactionsQuery",
      new Uint8Array(),
    );
    started[0]!.refuse(new Uint8Array([0, 1]));
    started[1]!.complete();

    expect(messages).toEqual([
      { kind: "chatQueryRefusal", queryId: 1, error: new Uint8Array([0, 1]) },
      { kind: "chatQueryEnd", queryId: 2 },
    ]);
    expect(queries.size).toBe(0);
  });

  it("reports a core that refuses to start a query", () => {
    const messages: WorkerToMain[] = [];
    const queries: ChatQueries = new Map();
    const core = fakeCore([], []);
    core.chatParticipantsQuery = () => {
      throw new Error("chat is not available");
    };

    handleChatQueryStart(
      core,
      (msg) => messages.push(msg),
      queries,
      1,
      3,
      "chatParticipantsQuery",
      new Uint8Array(),
    );
    handleChatQueryStart(
      undefined,
      (msg) => messages.push(msg),
      queries,
      2,
      4,
      "chatMessagesQuery",
      new Uint8Array(),
    );

    expect(messages).toEqual([
      { kind: "chatQueryFailure", queryId: 3, error: "chat is not available" },
      {
        kind: "chatQueryFailure",
        queryId: 4,
        error: "chatMessagesQuery received for unknown core 2",
      },
    ]);
    expect(queries.size).toBe(0);
  });

  it("stops a query once, and stops only the queries of a disposed core", () => {
    const log: string[] = [];
    const queries: ChatQueries = new Map();
    const core = fakeCore(log, []);
    const post = () => {};

    handleChatQueryStart(
      core,
      post,
      queries,
      1,
      1,
      "chatMessagesQuery",
      new Uint8Array(),
    );
    handleChatQueryStart(
      core,
      post,
      queries,
      2,
      2,
      "chatMessagesQuery",
      new Uint8Array(),
    );
    stopChatQuery(queries, 1);
    stopChatQuery(queries, 1);
    expect(log.filter((entry) => entry === "cancel")).toHaveLength(1);

    handleChatQueryStart(
      core,
      post,
      queries,
      1,
      3,
      "chatMessagesQuery",
      new Uint8Array(),
    );
    stopChatQueriesForCore(queries, 1);
    expect([...queries.keys()]).toEqual([2]);
  });
});

describe("worker wakes and staging", () => {
  it("answers the end of a hold, or no end when the budget is spent", () => {
    const messages: WorkerToMain[] = [];
    const log: string[] = [];
    const core = fakeCore(log, []);
    const post = (msg: WorkerToMain) => messages.push(msg);

    handleWakeProduct(core, post, 1, 3, "main", "dev");
    handleWakeProduct(core, post, 1, 4, "main", "spent");
    handleWakeProduct(undefined, post, 9, 5, undefined, undefined);

    expect(log).toEqual(["wake:main:dev", "wake:main:spent"]);
    expect(messages).toEqual([
      { kind: "wakeProductResponse", requestId: 3, ok: true, untilMs: 30_000 },
      {
        kind: "wakeProductResponse",
        requestId: 4,
        ok: true,
        untilMs: undefined,
      },
      {
        kind: "wakeProductResponse",
        requestId: 5,
        ok: false,
        error: "wakeProduct received for unknown core 9",
      },
    ]);
  });

  it("answers the refusal of a staging", async () => {
    const messages: WorkerToMain[] = [];
    await handleStageChatAttachment(
      fakeCore([], []),
      (msg) => messages.push(msg),
      1,
      6,
      "main",
      "image/png",
      new Uint8Array([1]),
    );
    expect(messages).toEqual([
      {
        kind: "stageChatAttachmentResponse",
        requestId: 6,
        ok: false,
        error: "this core does not stage chat attachments yet",
      },
    ]);
  });
});
