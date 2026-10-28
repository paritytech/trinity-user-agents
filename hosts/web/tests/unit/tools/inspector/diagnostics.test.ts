import { describe, expect, test } from "bun:test";
import {
  ChatDiagnostics,
  describeEvent,
  eventDirection,
  eventFields,
  eventOutcome,
  eventRecord,
  eventSearchText,
  eventSummary,
  isProblem,
  kindLabel,
  outcomeWord,
  relatedKeys,
} from "../../../../tools/inspector/diagnostics.js";
import {
  sanitizeReason,
  shortHex,
  shortId,
  subscriptionEvent,
  TOPIC_ID_LIMIT,
  type ChatEventBody,
} from "../../../../src/chat/nova/events.js";
import {
  CONTROL_TAGS,
  encodeMessage,
  messageKind,
} from "../../../../src/chat/nova/protocol.js";

describe("chat diagnostics", () => {
  test("keeps at most its limit, oldest dropped first", () => {
    const diagnostics = new ChatDiagnostics(3, () => 7);
    for (let index = 0; index < 5; index++)
      diagnostics.record({
        type: "set-aside",
        stream: "requests",
        reason: `r${index}`,
      });
    expect(diagnostics.list().map((event) => [event.seq, event.at])).toEqual([
      [3, 7],
      [4, 7],
      [5, 7],
    ]);
    expect(diagnostics.dropped()).toBe(2);
    diagnostics.clear();
    expect(diagnostics.list()).toEqual([]);
    expect(diagnostics.dropped()).toBe(0);
  });

  test("shortens hex runs and long text in reasons", () => {
    const key = `0x${"ab".repeat(32)}`;
    expect(sanitizeReason(`bad ${key} and ${"cd".repeat(20)}`)).toBe(
      "bad 0xabab…abab and 0xcdcd…cdcd",
    );
    expect(sanitizeReason("x".repeat(500)).length).toBe(201);
    expect(shortHex(key)).toBe("0xabab…abab");
    expect(shortId("123e4567-e89b-12d3-a456-426614174000")).toBe("123e4567…");
  });

  test("describes a received session request by its kinds", () => {
    expect(
      describeEvent({
        type: "session",
        peer: "0xee17…ad3b",
        data: "request",
        requestId: "1a2b3c4d…",
        kinds: [17, 1, null],
        duplicates: 1,
      }),
    ).toBe(
      "request 1a2b3c4d… from 0xee17…ad3b · kinds [17, 1, ?] · 1 already seen",
    );
  });

  test("names message kinds the protocol encodes, keeping the number", () => {
    const encode = (content: Parameters<typeof encodeMessage>[0]["content"]) =>
      messageKind(encodeMessage({ id: "m", timestampMs: 1n, content }));
    expect(kindLabel(encode({ kind: "text", text: "hi" }))).toBe("text (0)");
    expect(kindLabel(encode({ kind: "accepted", requestId: "r" }))).toBe(
      "chat accepted (14)",
    );
    expect(CONTROL_TAGS.map(kindLabel)).toEqual([
      "push token (1)",
      "device added (17)",
      "device removed (18)",
    ]);
    expect(kindLabel(99)).toBe("kind 99");
    expect(kindLabel(null)).toBe("unreadable");
  });

  test("summarizes a session request by runs of named kinds", () => {
    const event: ChatEventBody = {
      type: "session",
      peer: "0xee17…ad3b",
      data: "request",
      requestId: "4D4446B9…",
      kinds: [17, 17, 17, 1, null],
    };
    expect(eventSummary(event)).toBe(
      "request 4D4446B9… from 0xee17…ad3b · 5 messages: device added (17) ×3, push token (1), unreadable",
    );
    expect(eventDirection(event)).toBe("in");
    expect(eventOutcome(event)).toBe("info");
    // A search finds it by the name and by the raw number.
    expect(eventSearchText(event)).toContain("device added");
    expect(eventSearchText(event)).toContain("kinds [17, 17, 17, 1, ?]");
  });

  test("an acknowledged answer to our own request is a success", () => {
    const answer = (matched: boolean, code: number): ChatEventBody => ({
      type: "session",
      peer: "0xee17…ad3b",
      data: "response",
      requestId: "0fc45950…",
      code,
      matched,
    });
    expect(eventOutcome(answer(true, 0))).toBe("ok");
    expect(outcomeWord(answer(true, 0))).toBe("acknowledged");
    expect(eventOutcome(answer(false, 0))).toBe("info");
    expect(outcomeWord(answer(false, 0))).toBeNull();
    expect(eventSummary(answer(false, 0))).toBe(
      "response 0fc45950… from 0xee17…ad3b · acknowledged (0) · not ours",
    );
  });

  test("a refusal or data set aside warns, a failure is an error", () => {
    const submit = (result: "stored" | "refused" | "error"): ChatEventBody => ({
      type: "submit",
      purpose: "ack",
      peer: "0xee17…ad3b",
      result,
    });
    const events: ChatEventBody[] = [
      submit("stored"),
      submit("refused"),
      submit("error"),
      { type: "set-aside", stream: "sessions", reason: "unreadable" },
      { type: "lookup", account: "0x3c42…5b45", outcome: "missing" },
      { type: "lifecycle", state: "failed" },
    ];
    expect(
      events.map((event) => [
        eventOutcome(event),
        outcomeWord(event),
        isProblem(event),
      ]),
    ).toEqual([
      ["ok", "stored", false],
      ["warning", "refused", true],
      ["error", "error", true],
      ["warning", "set aside", true],
      ["warning", "missing", true],
      ["error", "failed", true],
    ]);
  });

  test("a subscription keeps its topics in full, only as 32-byte hex", () => {
    const diagnostics = new ChatDiagnostics(10, () => 0);
    const inbox = `0x${"ab".repeat(32)}`;
    diagnostics.record(subscriptionEvent("requests", "subscribe", [inbox]));
    const many = Array.from(
      { length: TOPIC_ID_LIMIT + 3 },
      (_, index) => `0x${index.toString(16).padStart(64, "0")}`,
    );
    diagnostics.record(
      subscriptionEvent("sessions", "subscribe", [
        "not a topic",
        `0x${"cd".repeat(31)}`,
        `0x${"EF".repeat(32)}`,
        ...many,
      ]),
    );
    const [requests, sessions] = diagnostics.list();
    expect(requests).toMatchObject({ topics: 1, topicIds: [inbox] });
    expect(eventSummary(requests)).toBe("subscribe requests (1 topic)");
    expect(eventFields(requests)).toEqual([
      { label: "Stream", value: "requests" },
      { label: "Action", value: "subscribe" },
      { label: "Topics", value: "1" },
      { label: "Request inbox topic", value: inbox },
      {
        label: "Topic is",
        value: 'blake2b-256("chat-request" ‖ identity account)',
      },
    ]);
    expect(eventSearchText(requests)).toContain(inbox);
    expect(eventRecord(requests).event).toMatchObject({ topicIds: [inbox] });

    // Anything that is not a 32-byte topic is dropped, and the list is bounded.
    expect(sessions.type === "subscription" && sessions.topicIds).toEqual([
      `0x${"ef".repeat(32)}`,
      ...many.slice(0, TOPIC_ID_LIMIT - 1),
    ]);
    expect(eventFields(sessions)[2]).toEqual({
      label: "Topics",
      value: `${TOPIC_ID_LIMIT + 6}, the first ${TOPIC_ID_LIMIT} kept`,
    });
  });

  test("links events by the shortened account and request id they carry", () => {
    expect(
      relatedKeys({
        type: "submit",
        purpose: "messages",
        peer: "0xee17…ad3b",
        requestId: "14a2e480…",
        result: "stored",
      }),
    ).toEqual({ account: "0xee17…ad3b", id: "14a2e480…" });
    expect(
      relatedKeys({ type: "lookup", account: "0x3c42…5b45", outcome: "found" }),
    ).toEqual({ account: "0x3c42…5b45" });
    expect(relatedKeys({ type: "lifecycle", state: "listening" })).toEqual({});
  });

  test("labels an event's fields and marks the ones that link events", () => {
    expect(
      eventFields({
        type: "lookup",
        account: "0x3c42…5b45",
        outcome: "found",
        from: ["people"],
      }),
    ).toEqual([
      { label: "Account", value: "0x3c42…5b45", related: "account" },
      { label: "Outcome", value: "found" },
      { label: "Chat key on", value: "people" },
    ]);
  });

  test("a copied event holds its metadata, with free text shortened again", () => {
    const diagnostics = new ChatDiagnostics(10, () => 0);
    const key = `0x${"ab".repeat(32)}`;
    diagnostics.record({
      type: "submit",
      purpose: "ack",
      peer: "0xee17…ad3b",
      result: "error",
      reason: `rejected ${key}`,
    });
    expect(eventRecord(diagnostics.list()[0])).toEqual({
      seq: 1,
      time: "1970-01-01T00:00:00.000Z",
      direction: "out",
      outcome: "error",
      event: {
        type: "submit",
        purpose: "ack",
        peer: "0xee17…ad3b",
        result: "error",
        reason: "rejected 0xabab…abab",
      },
    });
  });
});
