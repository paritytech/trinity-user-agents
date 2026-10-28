import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import { CallManager, STALE_OFFER_MS } from "../../../../src/chat/nova/call.js";
import type { OutgoingCallSignal } from "../../../../src/chat/nova/engine.js";
import { callLine } from "../../../../src/chat/nova-provider.js";

const ADA = `0x${"aa".repeat(32)}` as HexString;
const BOB = `0x${"bb".repeat(32)}` as HexString;

function manager(now = 1_000_000) {
  const sent: [HexString, OutgoingCallSignal][] = [];
  let opened = 0;
  const calls = new CallManager({
    send: async (account, signal) => {
      sent.push([account, signal]);
      return `id-${sent.length}`;
    },
    media: async () => {
      opened += 1;
      throw new DOMException("denied", "NotAllowedError");
    },
    rtc: { iceServers: [] },
    now: () => now,
  });
  return { calls, sent, opened: () => opened };
}

const offer = (account: HexString, id: string, at: number) => ({
  account,
  id,
  at,
  content: { kind: "call-offer" as const, sdp: Uint8Array.of(0), video: true },
});

describe("call manager", () => {
  test("an offer rings without opening anything, and a second one is answered as busy", () => {
    const { calls, sent, opened } = manager();
    calls.receive(offer(ADA, "o1", 1_000_000));
    expect(calls.current()?.session.state).toBe("incoming");
    expect(opened()).toBe(0);
    calls.receive(offer(BOB, "o2", 1_000_000));
    expect(sent).toEqual([[BOB, { kind: "call-closed", offerId: "o2" }]]);
    expect(calls.current()?.account).toBe(ADA);
  });

  test("an offer older than the ring window is closed, not rung", () => {
    const { calls, sent } = manager();
    calls.receive(offer(ADA, "o1", 1_000_000 - STALE_OFFER_MS - 1));
    expect(calls.current()).toBeNull();
    expect(sent).toEqual([[ADA, { kind: "call-closed", offerId: "o1" }]]);
  });

  test("the caller's close ends a ringing call as missed, and other calls' signals are ignored", async () => {
    const { calls } = manager();
    calls.receive(offer(ADA, "o1", 1_000_000));
    calls.receive({
      account: ADA,
      id: "c0",
      at: 1_000_000,
      content: { kind: "call-closed", offerId: "other" },
    });
    expect(calls.current()?.session.state).toBe("incoming");
    calls.receive({
      account: ADA,
      id: "c1",
      at: 1_000_000,
      content: { kind: "call-closed", offerId: "o1" },
    });
    await Promise.resolve();
    expect(calls.current()?.session.state).toBe("ended");
    expect(calls.current()?.session.problem).toBe("Missed call.");
    calls.clear();
    expect(calls.current()).toBeNull();
  });

  test("a denied camera keeps the call ringing with the reason, and declining tells the caller", async () => {
    const { calls, sent, opened } = manager();
    calls.receive(offer(ADA, "o1", 1_000_000));
    await calls.current()!.session.accept();
    expect(opened()).toBe(1);
    expect(calls.current()?.session.state).toBe("incoming");
    expect(calls.current()?.session.problem).toBe(
      "Camera or microphone access was denied.",
    );
    calls.current()!.session.hangUp();
    expect(sent).toEqual([[ADA, { kind: "call-closed", offerId: "o1" }]]);
  });

  test("an outgoing call whose media is denied sends nothing", async () => {
    const { calls, sent } = manager();
    await calls.start(ADA, false);
    expect(calls.current()?.session.state).toBe("ended");
    expect(sent).toEqual([]);
  });
});

describe("call lines", () => {
  test("say who called, how it ended, and for how long", () => {
    expect(callLine(true, { video: true })).toBe("Outgoing video call");
    expect(
      callLine(false, {
        video: false,
        answeredAt: 1_000,
        closedBy: "peer",
        endedAt: 66_000,
      }),
    ).toBe("Incoming audio call · 1:05");
    expect(callLine(true, { video: false, closedBy: "self", endedAt: 1 })).toBe(
      "Canceled audio call",
    );
    expect(callLine(true, { video: false, closedBy: "peer", endedAt: 1 })).toBe(
      "Declined audio call",
    );
    expect(callLine(false, { video: true, closedBy: "peer", endedAt: 1 })).toBe(
      "Missed video call",
    );
  });
});

describe("call relays", () => {
  test("a call asks for relays after the media opens and adds them to the configured servers", async () => {
    const configs: RTCConfiguration[] = [];
    const order: string[] = [];
    const calls = new CallManager({
      send: async () => "id",
      media: async () => {
        order.push("media");
        return { getTracks: () => [] } as unknown as MediaStream;
      },
      rtc: { iceServers: [{ urls: "stun:configured.invalid" }] },
      relays: async () => {
        order.push("relays");
        return [{ urls: "turn:relay.invalid", username: "u", credential: "c" }];
      },
      peerConnection: (config) => {
        configs.push(config);
        throw new Error("no peer connection in this test");
      },
      now: () => 0,
    });
    await calls.start(ADA, false);
    expect(order).toEqual(["media", "relays"]);
    expect(configs).toEqual([
      {
        iceServers: [
          { urls: "stun:configured.invalid" },
          { urls: "turn:relay.invalid", username: "u", credential: "c" },
        ],
      },
    ]);
  });
});
