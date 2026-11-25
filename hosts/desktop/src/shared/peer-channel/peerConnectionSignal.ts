/**
 * Phase-2 (media renegotiation) signaling payload, exchanged over the open
 * WebRTC data channel — NOT over the statement store. Byte parity with
 * Android `PeerConnectionSignal.kt` (Offer=0 / Answer=1 / IceCandidates=2)
 * and iOS `PeerConnectionSignal.swift`. Offer/Answer carry full, un-minimized
 * SDP text (no SdpCoder here — the channel budget is not statement-limited).
 *
 * Call teardown is sent as a Phase-1 chat `dataChannelClosed` message, not as
 * a signal here; there is deliberately no `closed` variant (Android parity).
 */

// `Nullable<T>` is an ambient global (globals.d.ts) — used bare, no import.
import { Enum, Option, Struct, Vector, str, u32 } from 'scale-ts';

export const PeerConnectionCandidateCodec = Struct({
  sdp: str,
  sdpMLineIndex: u32,
  sdpMid: Option(str),
});

export const PeerConnectionSignalCodec = Enum({
  offer: str, // 0
  answer: str, // 1
  candidates: Vector(PeerConnectionCandidateCodec), // 2
});

export type PeerConnectionCandidate = {
  sdp: string;
  sdpMLineIndex: number;
  sdpMid: Nullable<string>;
};

export type PeerConnectionSignal =
  { tag: 'offer'; value: string } | { tag: 'answer'; value: string } | { tag: 'candidates'; value: PeerConnectionCandidate[] };
