import { describe, expect, test } from "bun:test";
import { Reader } from "./codec.js";
import {
  candidateLine,
  compactBig,
  decodeCandidates,
  decodeChannelSignal,
  decodeFrame,
  decodeMediaState,
  decodeSetup,
  encodeCandidates,
  encodeChannelSignal,
  encodeFrame,
  encodeMediaState,
  encodeSetup,
  minimalCandidate,
  minimalSetup,
  readCompactBig,
  setupSdp,
  RENEGOTIATION,
} from "./call-wire.js";
import { decodeMessage, encodeMessage } from "./protocol.js";

const hex = (value: Uint8Array) => Buffer.from(value).toString("hex");

/** A first-stage offer as Chromium writes one, with synthetic credentials and a documentation address. */
const OFFER_SDP = [
  "v=0",
  "o=- 4611731400430051336 2 IN IP4 127.0.0.1",
  "s=-",
  "t=0 0",
  "a=group:BUNDLE 0",
  "a=extmap-allow-mixed",
  "a=msid-semantic: WMS",
  "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
  "c=IN IP4 0.0.0.0",
  "a=ice-ufrag:AbCd",
  "a=ice-pwd:0123456789abcdefghijklmn",
  "a=ice-options:trickle",
  "a=fingerprint:sha-256 0A:1B:2C:3D:4E:5F:60:71:82:93:A4:B5:C6:D7:E8:F9:0A:1B:2C:3D:4E:5F:60:71:82:93:A4:B5:C6:D7:E8:F9",
  "a=setup:actpass",
  "a=mid:0",
  "a=sctp-port:5000",
  "a=max-message-size:262144",
  "",
].join("\r\n");

describe("call signaling, in the apps' byte layout", () => {
  test("compact integers past 2^30 use the big-integer mode", () => {
    expect(hex(compactBig(1n))).toBe("04");
    expect(hex(compactBig(2n ** 30n))).toBe("0300000040");
    // 4611731400430051336 = 0x40002946_5225e008: eight bytes.
    const big = 4611731400430051336n;
    expect(hex(compactBig(big))).toBe(`13${"08e0255246290040"}`);
    expect(readCompactBig(new Reader(compactBig(big)))).toBe(big);
    expect(readCompactBig(new Reader(compactBig(300n)))).toBe(300n);
    expect(readCompactBig(new Reader(compactBig(70_000n)))).toBe(70_000n);
  });

  test("a minimal offer keeps the session, ICE credentials, fingerprint and candidates", () => {
    const candidate = minimalCandidate(
      "candidate:1467250027 1 udp 2122260223 192.0.2.10 56143 typ host generation 0 ufrag AbCd network-id 1",
    )!;
    expect(candidate).toEqual({
      foundation: "1467250027",
      priority: 2122260223,
      transport: "udp",
      address: "192.0.2.10",
      port: 56143,
      type: "host",
    });
    const setup = minimalSetup(OFFER_SDP, [candidate]);
    const encoded = encodeSetup(setup);
    expect(hex(encoded)).toBe(
      "00" + // offer
        "1308e0255246290040" + // session id
        "08" + // session version 2
        "1041624364" + // "AbCd"
        "60" +
        Buffer.from("0123456789abcdefghijklmn").toString("hex") +
        "80" +
        "0a1b2c3d4e5f60718293a4b5c6d7e8f9".repeat(2) + // fingerprint
        "04" + // one candidate
        "28" +
        Buffer.from("1467250027").toString("hex") +
        "ff1e7f7e" + // priority 2122260223 = 0x7e7f1eff
        "01" + // udp
        "00c000020a" + // IPv4 192.0.2.10
        "4fdb" + // port 56143
        "00", // host
    );
    expect(decodeSetup(encoded)).toEqual(setup);
  });

  test("the receiver rebuilds the SDP from the template, and candidates on mid 0", () => {
    const setup = minimalSetup(OFFER_SDP, []);
    expect(setupSdp(setup)).toBe(
      [
        "v=0",
        "o=- 4611731400430051336 2 IN IP4 0.0.0.0",
        "s=-",
        "t=0 0",
        "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
        "c=IN IP4 0.0.0.0",
        "a=ice-ufrag:AbCd",
        "a=ice-pwd:0123456789abcdefghijklmn",
        "a=fingerprint:sha-256 0A:1B:2C:3D:4E:5F:60:71:82:93:A4:B5:C6:D7:E8:F9:0A:1B:2C:3D:4E:5F:60:71:82:93:A4:B5:C6:D7:E8:F9",
        "a=setup:actpass",
        "a=mid:0",
        "a=sctp-port:5000",
        "",
      ].join("\r\n"),
    );
    expect(setupSdp({ ...setup, type: "answer" })).toContain("a=setup:active");
    const v6 = minimalCandidate(
      "candidate:9 1 UDP 100 2001:db8::1 3478 typ srflx raddr 0.0.0.0 rport 0",
    )!;
    const [back] = decodeCandidates(encodeCandidates([v6]));
    expect(candidateLine(back)).toBe(
      "candidate:9 1 UDP 100 2001:db8:0:0:0:0:0:1 3478 typ srflx",
    );
  });

  test("candidates with no compact form are left out", () => {
    expect(
      minimalCandidate(
        "candidate:1 1 udp 2122260223 4f1c-a2.local 5000 typ host",
      ),
    ).toBeNull();
    expect(
      minimalCandidate("candidate:1 2 udp 2122260223 192.0.2.1 5000 typ host"),
    ).toBeNull();
  });

  test("kinds 8 to 11 carry the setup, the purpose and the offer's id", () => {
    const head = "0872310807060504030201" + "00";
    const message = (content: Parameters<typeof encodeMessage>[0]["content"]) =>
      hex(
        encodeMessage({ id: "r1", timestampMs: 0x0102030405060708n, content }),
      );
    expect(
      message({ kind: "call-offer", sdp: Uint8Array.of(0xaa), video: true }),
    ).toBe(`${head}08${"04aa"}01`);
    expect(
      message({ kind: "call-answer", offerId: "o", sdp: Uint8Array.of(1) }),
    ).toBe(`${head}09046f0401`);
    expect(
      message({ kind: "call-candidates", offerId: "o", sdp: Uint8Array.of(1) }),
    ).toBe(`${head}0a046f0401`);
    expect(message({ kind: "call-closed", offerId: "o" })).toBe(
      `${head}0b046f`,
    );
    expect(
      decodeMessage(Buffer.from(`${head}08${"04aa"}00`, "hex")).content,
    ).toEqual({ kind: "call-offer", sdp: Uint8Array.of(0xaa), video: false });
  });

  test("the data channel frames a use case and its bytes, and carries the second negotiation", () => {
    const signal = encodeChannelSignal({ kind: "offer", sdp: "v=0" });
    expect(hex(signal)).toBe("000c763d30");
    const frame = encodeFrame(RENEGOTIATION, signal);
    expect(decodeFrame(frame)).toEqual({ id: RENEGOTIATION, data: signal });
    const candidates = {
      kind: "candidates" as const,
      candidates: [{ candidate: "candidate:x", sdpMLineIndex: 1, sdpMid: "1" }],
    };
    expect(decodeChannelSignal(encodeChannelSignal(candidates))).toEqual(
      candidates,
    );
    expect(hex(encodeMediaState({ camera: false }))).toBe("0000");
    expect(decodeMediaState(Uint8Array.of(1, 1))).toEqual({ microphone: true });
    expect(decodeChannelSignal(Uint8Array.of(3))).toBeNull();
  });
});
