import {
  Reader,
  bytes,
  compact,
  concat,
  list,
  option,
  readBytes,
  readList,
  readOption,
  text,
  u32,
} from "./codec.js";

/**
 * The Polkadot app's call signaling, written from the app's protocol
 * (Android `tools/media-connection`, iOS `Modules/ChatCall`, vendored under
 * `hosts/`). No code is taken from them.
 *
 * A call is two WebRTC negotiations. The first opens a data channel: its
 * offer and answer travel as chat messages (kinds 8 and 9) carrying a
 * compact form of the SDP, the "minimal setup", that keeps only the ICE
 * credentials, the DTLS fingerprint and some candidates; the receiver
 * rebuilds an SDP from a fixed template. More candidates follow as kind 10.
 * Once the channel is open, the caller adds audio and video and negotiates
 * again over the channel with whole SDP strings, framed per use case. Kind
 * 11 ends the call, from either side, in any state.
 */

// ---------------------------------------------------------------------------
// Compact integers above 2^30, for the SDP session id and version
// ---------------------------------------------------------------------------

/** A SCALE compact integer of any size. */
export function compactBig(value: bigint): Uint8Array {
  if (value < 0n) throw new RangeError("a compact integer is not negative");
  if (value < 1n << 30n) return compact(Number(value));
  const out: number[] = [];
  for (let rest = value; rest > 0n; rest >>= 8n) out.push(Number(rest & 0xffn));
  return Uint8Array.of(((out.length - 4) << 2) | 3, ...out);
}

export function readCompactBig(reader: Reader): bigint {
  const first = reader.take(1)[0];
  if ((first & 3) !== 3) {
    const width = [1, 2, 4][first & 3];
    let value = BigInt(first);
    const more = reader.take(width - 1);
    more.forEach(
      (byte, index) => (value |= BigInt(byte) << BigInt(8 * (index + 1))),
    );
    return value >> 2n;
  }
  const length = (first >> 2) + 4;
  let value = 0n;
  reader.take(length).forEach((byte, index) => {
    value |= BigInt(byte) << BigInt(8 * index);
  });
  return value;
}

// ---------------------------------------------------------------------------
// Minimal setup and candidates
// ---------------------------------------------------------------------------

const TRANSPORTS = ["tcp", "udp"] as const;
const CANDIDATE_TYPES = ["host", "srflx", "relay", "prflx"] as const;

/** One ICE candidate in the compact form: component 1, media line 0, no extensions. */
export interface MinimalCandidate {
  foundation: string;
  priority: number;
  transport: (typeof TRANSPORTS)[number];
  /** Dotted IPv4 or colon IPv6, as numbers. A name, such as an mDNS one, has no compact form. */
  address: string;
  port: number;
  type: (typeof CANDIDATE_TYPES)[number];
}

export interface MinimalSetup {
  type: "offer" | "answer";
  sessionId: bigint;
  sessionVersion: bigint;
  iceUfrag: string;
  icePwd: string;
  /** The SHA-256 DTLS fingerprint, raw. */
  fingerprint: Uint8Array;
  candidates: MinimalCandidate[];
}

function encodeAddress(address: string): Uint8Array {
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(address);
  if (v4 !== null) {
    const parts = v4.slice(1).map(Number);
    if (parts.some((part) => part > 255))
      throw new Error("not an IPv4 address");
    return Uint8Array.of(0, ...parts);
  }
  const groups = expandIpv6(address);
  return concat(
    Uint8Array.of(1),
    ...groups.map((group) => Uint8Array.of(group & 0xff, group >> 8)),
  );
}

function expandIpv6(address: string): number[] {
  if (!/^[0-9a-fA-F:]+$/.test(address) || !address.includes(":"))
    throw new Error(`${address} is not an IP address`);
  const [head, tail, extra] = address.split("::");
  if (extra !== undefined) throw new Error(`${address} is not an IP address`);
  const read = (part: string | undefined) =>
    part === undefined || part === "" ? [] : part.split(":");
  const left = read(head);
  const right = read(tail);
  const fill = tail === undefined ? 0 : 8 - left.length - right.length;
  const groups = [...left, ...Array(fill).fill("0"), ...right];
  if (groups.length !== 8 || groups.some((group) => group.length > 4))
    throw new Error(`${address} is not an IP address`);
  return groups.map((group) => parseInt(group, 16));
}

function readAddress(reader: Reader): string {
  const tag = reader.u8();
  if (tag === 0) return [...reader.take(4)].join(".");
  if (tag !== 1) throw new Error(`the address is tag ${tag}`);
  const groups = Array.from({ length: 8 }, () => {
    const pair = reader.take(2);
    return (pair[0] | (pair[1] << 8)).toString(16);
  });
  return groups.join(":");
}

const u16 = (value: number) => Uint8Array.of(value & 0xff, value >> 8);

function encodeCandidate(candidate: MinimalCandidate): Uint8Array {
  return concat(
    text(candidate.foundation),
    u32(candidate.priority),
    Uint8Array.of(TRANSPORTS.indexOf(candidate.transport)),
    encodeAddress(candidate.address),
    u16(candidate.port),
    Uint8Array.of(CANDIDATE_TYPES.indexOf(candidate.type)),
  );
}

function readCandidate(reader: Reader): MinimalCandidate {
  const foundation = reader.text();
  const priority = reader.u32();
  const transport = TRANSPORTS[reader.u8()];
  const address = readAddress(reader);
  const port = reader.take(2);
  const type = CANDIDATE_TYPES[reader.u8()];
  if (transport === undefined || type === undefined)
    throw new Error("the candidate names an unknown transport or type");
  return {
    foundation,
    priority,
    transport,
    address,
    port: port[0] | (port[1] << 8),
    type,
  };
}

export function encodeSetup(setup: MinimalSetup): Uint8Array {
  return concat(
    Uint8Array.of(setup.type === "offer" ? 0 : 1),
    compactBig(setup.sessionId),
    compactBig(setup.sessionVersion),
    text(setup.iceUfrag),
    text(setup.icePwd),
    bytes(setup.fingerprint),
    list(setup.candidates, encodeCandidate),
  );
}

export function decodeSetup(encoded: Uint8Array): MinimalSetup {
  const reader = new Reader(encoded);
  const type = reader.u8();
  if (type > 1) throw new Error(`the setup is type ${type}`);
  const setup: MinimalSetup = {
    type: type === 0 ? "offer" : "answer",
    sessionId: readCompactBig(reader),
    sessionVersion: readCompactBig(reader),
    iceUfrag: reader.text(),
    icePwd: reader.text(),
    fingerprint: readBytes(reader),
    candidates: readList(reader, readCandidate),
  };
  reader.finish();
  return setup;
}

export const encodeCandidates = (candidates: MinimalCandidate[]): Uint8Array =>
  list(candidates, encodeCandidate);

export function decodeCandidates(encoded: Uint8Array): MinimalCandidate[] {
  const reader = new Reader(encoded);
  const candidates = readList(reader, readCandidate);
  reader.finish();
  return candidates;
}

/**
 * The compact form of a `candidate:` line, or null when it has none: a
 * component other than 1, or an address that is a name, as browsers give
 * for mDNS. Extensions such as `raddr` are dropped, as the apps drop them.
 */
export function minimalCandidate(line: string): MinimalCandidate | null {
  const parts = line
    .trim()
    .replace(/^a=/, "")
    .replace(/^candidate:/, "")
    .split(/\s+/);
  if (parts.length < 8 || parts[1] !== "1" || parts[6] !== "typ") return null;
  const transport = parts[2].toLowerCase();
  const type = parts[7].toLowerCase();
  const priority = Number(parts[3]);
  const port = Number(parts[5]);
  if (
    !(TRANSPORTS as readonly string[]).includes(transport) ||
    !(CANDIDATE_TYPES as readonly string[]).includes(type) ||
    !Number.isInteger(priority) ||
    priority < 0 ||
    priority >= 2 ** 31 ||
    !Number.isInteger(port) ||
    port < 0 ||
    port > 0xffff
  )
    return null;
  try {
    encodeAddress(parts[4]);
  } catch {
    return null;
  }
  return {
    foundation: parts[0],
    priority,
    transport: transport as MinimalCandidate["transport"],
    address: parts[4],
    port,
    type: type as MinimalCandidate["type"],
  };
}

/** The `candidate:` line a compact candidate stands for, on media line 0, mid `0`. */
export const candidateLine = (candidate: MinimalCandidate): string =>
  `candidate:${candidate.foundation} 1 ${candidate.transport.toUpperCase()} ${candidate.priority} ${candidate.address} ${candidate.port} typ ${candidate.type}`;

const sdpValue = (sdp: string, prefix: string): string | null =>
  sdp
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.startsWith(prefix))
    ?.slice(prefix.length) ?? null;

/** Read a first-stage SDP's minimal setup. Throws when it lacks what the setup needs. */
export function minimalSetup(
  sdp: string,
  candidates: MinimalCandidate[],
): MinimalSetup {
  const origin = sdpValue(sdp, "o=")?.split(/\s+/) ?? [];
  const iceUfrag = sdpValue(sdp, "a=ice-ufrag:");
  const icePwd = sdpValue(sdp, "a=ice-pwd:");
  const fingerprint = sdpValue(sdp, "a=fingerprint:");
  const setup = sdpValue(sdp, "a=setup:");
  if (origin.length < 3 || iceUfrag === null || icePwd === null)
    throw new Error("the SDP has no session or ICE credentials");
  if (fingerprint === null || !/^sha-256 /i.test(fingerprint))
    throw new Error("the SDP has no SHA-256 fingerprint");
  const hex = fingerprint.split(/\s+/)[1].replaceAll(":", "");
  return {
    type: setup === "actpass" ? "offer" : "answer",
    sessionId: BigInt(origin[1]),
    sessionVersion: BigInt(origin[2]),
    iceUfrag,
    icePwd,
    fingerprint: Uint8Array.from(hex.match(/../g) ?? [], (pair) =>
      parseInt(pair, 16),
    ),
    candidates,
  };
}

/** The SDP a minimal setup stands for, from the template both apps rebuild it with. */
export function setupSdp(setup: MinimalSetup): string {
  const fingerprint = [...setup.fingerprint]
    .map((byte) => byte.toString(16).toUpperCase().padStart(2, "0"))
    .join(":");
  return [
    "v=0",
    `o=- ${setup.sessionId} ${setup.sessionVersion} IN IP4 0.0.0.0`,
    "s=-",
    "t=0 0",
    "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
    "c=IN IP4 0.0.0.0",
    `a=ice-ufrag:${setup.iceUfrag}`,
    `a=ice-pwd:${setup.icePwd}`,
    `a=fingerprint:sha-256 ${fingerprint}`,
    `a=setup:${setup.type === "offer" ? "actpass" : "active"}`,
    "a=mid:0",
    "a=sctp-port:5000",
    "",
  ].join("\r\n");
}

// ---------------------------------------------------------------------------
// The data channel
// ---------------------------------------------------------------------------

/** The use cases the data channel carries. */
export const RENEGOTIATION = "webrtc_renegotiation_internal_use_case";
export const MEDIA_STATE = "webrtc_media_state_use_case";

/** One frame on the data channel: `{ id: String, data: Vec<u8> }`. */
export const encodeFrame = (id: string, data: Uint8Array): Uint8Array =>
  concat(text(id), bytes(data));

export function decodeFrame(encoded: Uint8Array): {
  id: string;
  data: Uint8Array;
} {
  const reader = new Reader(encoded);
  const frame = { id: reader.text(), data: readBytes(reader) };
  reader.finish();
  return frame;
}

/** The second negotiation, over the channel, with whole SDP strings. */
export type ChannelSignal =
  | { kind: "offer"; sdp: string }
  | { kind: "answer"; sdp: string }
  | {
      kind: "candidates";
      candidates: {
        candidate: string;
        sdpMLineIndex: number;
        sdpMid: string | null;
      }[];
    };

export function encodeChannelSignal(signal: ChannelSignal): Uint8Array {
  switch (signal.kind) {
    case "offer":
      return concat(Uint8Array.of(0), text(signal.sdp));
    case "answer":
      return concat(Uint8Array.of(1), text(signal.sdp));
    case "candidates":
      return concat(
        Uint8Array.of(2),
        list(signal.candidates, (entry) =>
          concat(
            text(entry.candidate),
            u32(entry.sdpMLineIndex),
            option(entry.sdpMid, text),
          ),
        ),
      );
  }
}

/** Decode a channel signal, or null for one this host does not read. */
export function decodeChannelSignal(encoded: Uint8Array): ChannelSignal | null {
  const reader = new Reader(encoded);
  const tag = reader.u8();
  if (tag === 0) return { kind: "offer", sdp: reader.text() };
  if (tag === 1) return { kind: "answer", sdp: reader.text() };
  if (tag === 2)
    return {
      kind: "candidates",
      candidates: readList(reader, (inner) => ({
        candidate: inner.text(),
        sdpMLineIndex: inner.u32(),
        sdpMid: readOption(inner, (value) => value.text()),
      })),
    };
  return null;
}

/** Whether the sender's camera or microphone is on: `0` camera, `1` microphone, then a bool. */
export type MediaStateSignal = { camera: boolean } | { microphone: boolean };

export const encodeMediaState = (signal: MediaStateSignal): Uint8Array =>
  "camera" in signal
    ? Uint8Array.of(0, signal.camera ? 1 : 0)
    : Uint8Array.of(1, signal.microphone ? 1 : 0);

export function decodeMediaState(encoded: Uint8Array): MediaStateSignal | null {
  if (encoded.length !== 2 || encoded[1] > 1) return null;
  if (encoded[0] === 0) return { camera: encoded[1] === 1 };
  if (encoded[0] === 1) return { microphone: encoded[1] === 1 };
  return null;
}
