import { describe, expect, test } from "bun:test";

import * as W from "@parity/truapi/wire-table";
import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";

import { createFrameDecoder, type FrameValueDetail } from "./decode.js";
import type { ObservedFrame } from "./observed-frame.js";
import { frameIdOf } from "./wire-debugger.js";

/** A minimal observed frame for a given id/bytes; the fields decode ignores are stubbed. */
function frame(frameId: number, bytes?: Uint8Array, messageType = 0): ObservedFrame {
  return {
    channelId: "myapp.dot",
    direction: "out",
    requestId: "p:1",
    frameId,
    messageType,
    role: "unknown",
    byteLength: bytes?.length ?? 0,
    timestamp: 0,
    // These tests exercise the DECODER. Decode is gated on the frame's producer
    // having vouched for the wire contract, so an attested frame is the fixture;
    // `unattested()` below covers the refusal.
    //
    // NOTE this INVERTS the production default: on the wire the field is absent
    // and absent means untrusted. A new test reaching for `frame()` silently opts
    // into trust, so anything asserting a refusal must start from `unattested()`.
    identityConfirmed: true,
    ...(bytes ? { bytes } : {}),
  };
}

/** The same frame with no identity: its producer never vouched for the contract. */
function unattested(frameId: number, bytes?: Uint8Array): ObservedFrame {
  const f = frame(frameId, bytes);
  delete f.identityConfirmed;
  return f;
}

describe("frame decoder (real table) — decodes everything, no special-casing", () => {
  test("a non-sensitive frame decodes only with the toggle on", () => {
    // `connection-status.subscribe` takes no request at all, so its Start leg
    // (messageType 0, the frame's default) decodes to bare `undefined` - a real
    // frame the generated table can decode, ignoring these two arbitrary bytes.
    const id = frameIdOf(
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.trait,
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.method,
    );
    const bytes = new Uint8Array([0, 0]);

    const off = createFrameDecoder({ enabled: false });
    const offDetail = off.detail(frame(id, bytes));
    expect(offDetail.kind).toBe("bytes");
    if (offDetail.kind === "bytes") expect(offDetail.byteLength).toBe(2);

    const on = createFrameDecoder({ enabled: true });
    const onDetail = on.detail(frame(id, bytes));
    expect(onDetail.kind).toBe("decoded");
    // Sanity: the id's Start leg really is in the generated decode table.
    expect(typeof WIRE_DECODE_TABLE[id]?.[0]).toBe("function");
  });

  test("a formerly-'sensitive' signing frame decodes too (dev-only tool)", () => {
    // No denylist any more: a signing request decodes like every other frame.
    const decoder = createFrameDecoder({ enabled: true });
    const detail = decoder.detail(
      frame(
        frameIdOf(W.SIGNING_SIGN_RAW.trait, W.SIGNING_SIGN_RAW.method),
        new Uint8Array([0]),
      ),
    );
    // It either decodes (id has a codec + valid bytes) or, on a codec throw for
    // the stub bytes, falls back to bytes — never a "redacted" state.
    expect(["decoded", "bytes"]).toContain(detail.kind);
    // Whatever the outcome, the kind is never the old "redacted" variant.
    expect(detail.kind).not.toBe("redacted");
  });

  test("disabled decoder is bytes-only for every frame", () => {
    const decoder = createFrameDecoder({ enabled: false });
    for (const id of [
      frameIdOf(W.ACCOUNT_GET_ACCOUNT.trait, W.ACCOUNT_GET_ACCOUNT.method),
      frameIdOf(W.SIGNING_SIGN_RAW.trait, W.SIGNING_SIGN_RAW.method),
      frameIdOf(W.CHAIN_CALL_HEAD.trait, W.CHAIN_CALL_HEAD.method),
    ]) {
      expect(decoder.detail(frame(id, new Uint8Array([9]))).kind).toBe("bytes");
    }
  });
});

describe("frame decoder (injected table)", () => {
  const table = { 999: { 0: (b: Uint8Array) => ({ ok: Array.from(b) }) } };

  test("decodes an id when enabled and bytes present", () => {
    const decoder = createFrameDecoder({ enabled: true, decodeTable: table });
    const detail = decoder.detail(frame(999, new Uint8Array([1, 2])));
    expect(detail).toEqual({
      kind: "decoded",
      value: { ok: [1, 2] },
    } satisfies FrameValueDetail);
  });

  test("decodes a secret-named field too — no content guard withholds it", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 999: { 0: () => ({ source: { sr25519SecretKey: "0xdead" } }) } },
    });
    const detail = decoder.detail(frame(999, new Uint8Array([1])));
    expect(detail.kind).toBe("decoded");
    if (detail.kind === "decoded") {
      expect(detail.value).toEqual({ source: { sr25519SecretKey: "0xdead" } });
    }
  });

  test("falls back to bytes when the frame retained no bytes", () => {
    const decoder = createFrameDecoder({ enabled: true, decodeTable: table });
    expect(decoder.detail(frame(999)).kind).toBe("bytes");
  });

  test("falls back to bytes when the codec throws", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: {
        999: {
          0: () => {
            throw new Error("bad payload");
          },
        },
      },
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe("bytes");
  });

  test("falls back to bytes when the id has no codec", () => {
    const decoder = createFrameDecoder({ enabled: true, decodeTable: table });
    expect(decoder.detail(frame(1, new Uint8Array([1]))).kind).toBe("bytes");
  });
});

test("an unattested frame never decodes, whatever its channel did", () => {
  // The gate is per FRAME, not per channel. As a per-channel latch, one attested
  // frame retroactively unlocked every unattested frame already retained under
  // the same `channelId` - and `channelId` is the productId, shared by a stale
  // host and a fresh one, and by every frame an embedding host tees before it
  // learns its core's schema hash.
  const decoder = createFrameDecoder({ enabled: true });
  const id = frameIdOf(
    W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.trait,
    W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.method,
  );
  const bytes = new Uint8Array([0, 0]);

  // Same id, same bytes. The only difference is who vouched for the contract.
  const attested = decoder.detail(frame(id, bytes));
  const refused = decoder.detail(unattested(id, bytes));

  expect(attested.kind).toBe("decoded");
  expect(refused.kind).toBe("bytes");
  if (refused.kind === "bytes") expect(refused.hex).toBe("0x0000");
});
