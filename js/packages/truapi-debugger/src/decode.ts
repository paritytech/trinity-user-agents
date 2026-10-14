// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Level-2 decode: turn a frame's raw SCALE payload into a plain JS value, in the
 * drill-down detail path.
 *
 * This is the one place the debugger looks *inside* a frame. Everything else -
 * the trace engine, `/traces`, the host tap - is payload-blind and stays that
 * way. The rules that make that work live here:
 *
 *  - **Dev-only tool: decode everything.** This debugger decodes every frame it
 *    can, with no "sensitive" special-casing. A developer inspecting their own
 *    session's traffic sees the real values. When decoding is disabled every
 *    frame reports its byte length only.
 *  - **Reuse, don't reinvent.** Decoding is
 *    `WIRE_DECODE_TABLE[frameId]?.[messageType]?.(bytes)` from
 *    `@parity/truapi/wire-decode` - the same generated, dev-only codecs the
 *    client uses, keyed by the wire's own leg marker. The debugger writes no
 *    codecs of its own.
 *
 * Nothing here is ever serialized into `/traces`; the detail it produces is
 * returned only from the explicit per-frame drill-down.
 *
 * @module
 */

import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";
import type { ObservedFrame } from "./observed-frame.js";

/**
 * Per-frame decode result for the drill-down detail path.
 *
 * `"decoded"` carries the plain JS value, returned whenever the decoder is on
 * and the frame's id has a codec that decodes its retained bytes. `"bytes"` is
 * the fallback: the decoder is off, the frame carries no retained bytes, its id
 * has no codec, or decoding threw. When the decoder is on and the bytes are
 * retained, that fallback still carries the raw `hex` so a dev-only tool always
 * shows *something* for a payload it could not type; `hex` is absent only in
 * payload-blind mode (decoder off) or when no bytes were retained.
 */
export type FrameValueDetail =
  | { kind: "decoded"; value: unknown }
  | { kind: "bytes"; byteLength: number; hex?: string };

/** Options for {@link createFrameDecoder}. */
export interface FrameDecoderOptions {
  /**
   * Master gate. `false` (the default) means the decoder never inspects a
   * payload: every frame reports bytes only.
   */
  enabled?: boolean;
  /**
   * Frame-id → messageType → decoder map. Defaults to the generated
   * {@link WIRE_DECODE_TABLE}; overridable for tests.
   */
  decodeTable?: Record<number, Record<number, (payload: Uint8Array) => unknown>>;
}

/** A gated per-frame value decoder for the drill-down detail path. */
export interface FrameDecoder {
  /** Whether decoding is on. `false` ⇒ every `detail` is bytes-only. */
  readonly enabled: boolean;
  /** Resolve one frame to its {@link FrameValueDetail}. */
  detail(frame: ObservedFrame): FrameValueDetail;
}

/**
 * Build a {@link FrameDecoder}. Off by default: pass `enabled: true` to opt in.
 * When on, every frame with a codec and retained bytes decodes to its value.
 */
export function createFrameDecoder(
  options: FrameDecoderOptions = {},
): FrameDecoder {
  const enabled = options.enabled ?? false;
  const decodeTable = options.decodeTable ?? WIRE_DECODE_TABLE;

  // Raw bytes as `0x…` hex so a payload the decoder can't type is still visible
  // in the drill-down (a dev-only tool hides nothing it has the bytes for).
  const toHex = (bytes: Uint8Array): string =>
    "0x" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

  const detail = (frame: ObservedFrame): FrameValueDetail => {
    if (!enabled) return { kind: "bytes", byteLength: frame.byteLength };
    // Decoder on: keep the raw hex on the bytes fallback so nothing reads
    // "payload not shown" when the bytes are right there.
    const bytesFallback = (): FrameValueDetail => ({
      kind: "bytes",
      byteLength: frame.byteLength,
      ...(frame.bytes ? { hex: toHex(frame.bytes) } : {}),
    });
    // The frame's OWN producer must have vouched for the wire contract. This is
    // ADDITIVE to the per-channel `decodeTrusted` gate both mounts still apply,
    // not a replacement for it. Keying on the channel ALONE made it a latch:
    // one attested frame unlocked every unattested frame already retained under
    // that `channelId`, and a frame id means nothing without the table that
    // assigned it. Unattested frames still group, list and show their hex.
    if (frame.identityConfirmed !== true) return bytesFallback();
    const decode = decodeTable[frame.frameId]?.[frame.messageType];
    if (!decode || !frame.bytes) return bytesFallback();
    try {
      return { kind: "decoded", value: decode(frame.bytes) };
    } catch {
      // A malformed or version-skewed payload must not break the drill-down;
      // fall back to the raw hex.
      return bytesFallback();
    }
  };

  return { enabled, detail };
}
