import { describe, expect, test } from "bun:test";
import { EventLog, toLogValue } from "../../worker/log";
import { decodeState, encodeState, initialState } from "../../worker/state";

describe("EventLog", () => {
  test("writes one JSON line per entry with the fields of the spec", () => {
    const lines: string[] = [];
    const log = new EventLog(1, (line) => lines.push(line));
    const seq = log.record({
      via: "user_action",
      room: "source",
      kind: "NewMessage",
      data: { body: { tag: "Text", value: { text: "/help" } }, replyTo: undefined },
    });
    log.record({ via: "answer", re: seq, room: "source", kind: "MessageCreated", data: { messageId: "source-u-0003", timestamp: 5n } });
    expect(lines).toEqual([
      '{"t":"chat-v2","boot":1,"seq":1,"via":"user_action","room":"source","kind":"NewMessage","data":{"body":{"tag":"Text","value":{"text":"/help"}},"replyTo":null}}',
      '{"t":"chat-v2","boot":1,"seq":2,"via":"answer","re":1,"room":"source","kind":"MessageCreated","data":{"messageId":"source-u-0003","timestamp":"5"}}',
    ]);
  });

  test("bytes become hex and an absent value becomes null", () => {
    expect(toLogValue({ a: new Uint8Array([1, 255]), b: [1n, undefined], c: undefined })).toEqual({ a: "0x01ff", b: ["1", null], c: null });
    const lines: string[] = [];
    new EventLog(2, (line) => lines.push(line)).record({ via: "action_subscribe", room: "host", kind: "RoomOpened", data: undefined });
    expect(JSON.parse(lines[0])).toEqual({ t: "chat-v2", boot: 2, seq: 1, via: "action_subscribe", room: "host", kind: "RoomOpened", data: null });
  });
});

describe("state", () => {
  test("keeps bigint values across a save", () => {
    const state = initialState();
    state.rooms.host.preview = { order: "PinToTop", timestamp: 1_767_225_600_000n };
    state.source.live.push({
      message: {
        messageId: "source-p-0001",
        origin: { participantId: "product" },
        timestamp: 1_767_225_601_000n,
        body: { tag: "Text", value: { text: "x" } },
        status: "Sent",
        editedAt: 9n,
        reactions: [],
      },
      reactions: [],
    });
    expect(decodeState(encodeState(state))).toEqual(state);
  });

  test("refuses a state of another version", () => {
    expect(() => decodeState('{"version":1}')).toThrow();
  });
});
