import { bytesToHex } from "@parity/truapi/scale";
import { Reader } from "./codec.js";
import { sanitizeReason } from "./diagnostics.js";
import { ACKNOWLEDGED } from "./protocol.js";
import { statementHash } from "./statement.js";

/**
 * The inspector's Decoded layer: captured bytes read field by field, kept in
 * two parts.
 *
 * - `protocol`: the values parsed from the bytes, in wire order. The binary
 *   has no names, so keys are the schema's field names: the statement store's
 *   `Statement` and the Polkadot app's chat types (Android
 *   `feature/chats/transport-protocol`, `ChatRequestRemoteModel.kt` and
 *   `StructuredStatementData.kt`, vendored under `hosts/android`). Enum
 *   indexes, option tags, list counts and length prefixes are kept as read;
 *   a wrapper type that adds no bytes is left out.
 *
 *   SCALE shapes: `String` is `{ length, utf8 }`, `Vec<u8>` `{ length, bytes }`,
 *   `Option<T>` `{ option, value? }`, a list `{ count, items }`, an enum
 *   `{ index, ...fields }`, a fixed byte array hex, a `u64` decimal text.
 * - `annotations`: what this host adds, keyed by the path of the protocol
 *   value it describes (`$` is the layer's root): names of enum indexes, times,
 *   hashes, sizes, offsets, notes, and where and why reading stopped, with the
 *   bytes it did not read.
 */

export type Json =
  | string
  | number
  | boolean
  | null
  | Json[]
  | { [key: string]: Json };

type Fields = { [key: string]: Json };

/** Annotations, keyed by protocol path. */
export type Annotations = Record<string, Fields>;

export interface Decoded {
  protocol: Fields;
  annotations: Annotations;
}

/** Why reading stops: an error, or a shape this host does not read. */
class Stop extends Error {
  constructor(
    message: string,
    readonly kind: "error" | "not read",
  ) {
    super(message);
  }
}

const notRead = (reason: string) => new Stop(reason, "not read");
const invalid = (reason: string) => new Stop(reason, "error");

const hex = (bytes: Uint8Array): string => bytesToHex(bytes);

const errorText = (error: unknown): string =>
  sanitizeReason(error instanceof Error ? error.message : String(error));

class Walk {
  readonly annotations: Annotations = {};
  /** The protocol path being read, for where reading stopped. */
  path = "$";

  constructor(readonly reader: Reader) {}

  note(path: string, values: Fields): void {
    this.annotations[path] = { ...this.annotations[path], ...values };
  }

  private at(path: string): Reader {
    this.path = path;
    return this.reader;
  }

  u8(path: string): number {
    return this.at(path).u8();
  }

  compact(path: string): number {
    return this.at(path).compact();
  }

  u32(path: string): number {
    return this.at(path).u32();
  }

  u64(path: string): string {
    return this.at(path).u64().toString();
  }

  take(path: string, length: number): Uint8Array {
    return this.at(path).take(length);
  }

  fixed(path: string, length: number): string {
    return hex(this.take(path, length));
  }

  /** A `String`, placed in `out` as soon as its length is read. */
  string(out: Fields, key: string, path: string): void {
    const node: Fields = {};
    out[key] = node;
    this.stringInto(node, path);
  }

  /** A `String` read into `node`. */
  stringInto(node: Fields, path: string): void {
    node.length = this.compact(`${path}.length`);
    const bytes = this.take(`${path}.utf8`, node.length);
    try {
      node.utf8 = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw invalid("the bytes are not UTF-8");
    }
  }

  /** A `Vec<u8>`, placed in `out` as soon as its length is read. */
  vec(out: Fields, key: string, path: string): Uint8Array {
    const node: Fields = { length: this.compact(`${path}.length`) };
    out[key] = node;
    const bytes = this.take(`${path}.bytes`, node.length as number);
    node.bytes = hex(bytes);
    return bytes;
  }

  /** An `Option<T>`: `read` fills the value's fields. */
  option(
    out: Fields,
    key: string,
    path: string,
    read: (value: Fields, path: string) => void,
  ): void {
    const node: Fields = { option: this.u8(`${path}.option`) };
    out[key] = node;
    if (node.option === 0) return;
    if (node.option !== 1)
      throw invalid(`option tag ${node.option} is neither 0 nor 1`);
    const value: Fields = {};
    node.value = value;
    read(value, `${path}.value`);
  }

  /** An enum's index, with its name from `names` as an annotation. */
  index(out: Fields, path: string, names: Record<number, string>): number {
    const index = this.u8(`${path}.index`);
    out.index = index;
    const name = names[index];
    if (name !== undefined) this.note(`${path}.index`, { name });
    return index;
  }

  /** Read `bytes` with `read` into `out`, keeping its annotations under `path`. */
  nested(
    out: Fields,
    path: string,
    bytes: Uint8Array,
    read: (walk: Walk, out: Fields) => void,
  ): void {
    const inner = decode(bytes, read, false);
    Object.assign(out, inner.protocol);
    const outer = (innerPath: string) => path + innerPath.slice(1);
    for (const [innerPath, values] of Object.entries(inner.annotations)) {
      const stopped = values.stopped as Fields | undefined;
      this.note(
        outer(innerPath),
        stopped === undefined
          ? values
          : {
              ...values,
              stopped: {
                ...stopped,
                path: outer(stopped.path as string),
                within: path,
              },
            },
      );
    }
  }
}

/** Read `bytes` with `read`, noting at `$` whether all of them were read. */
function decode(
  bytes: Uint8Array,
  read: (walk: Walk, out: Fields) => void,
  withSize = true,
): Decoded {
  const walk = new Walk(new Reader(bytes));
  const protocol: Fields = {};
  const size: Fields = withSize ? { size: bytes.length } : {};
  try {
    read(walk, protocol);
    const left = bytes.length - walk.reader.offset;
    walk.path = "$";
    if (left > 0)
      throw invalid(
        `${left} byte${left === 1 ? "" : "s"} after the last field`,
      );
    const unfinished = Object.entries(walk.annotations)
      .filter(([, values]) => values.complete === false)
      .map(([path]) => path);
    walk.note(
      "$",
      unfinished.length === 0
        ? { ...size, complete: true }
        : { ...size, complete: false, incompleteAt: unfinished },
    );
  } catch (error) {
    const at = walk.reader.offset;
    walk.note("$", {
      ...size,
      complete: false,
      stopped: {
        path: walk.path,
        at,
        within: "$",
        kind: error instanceof Stop ? error.kind : "error",
        reason: errorText(error),
        unread: hex(bytes.subarray(at)),
      },
    });
  }
  return { protocol, annotations: walk.annotations };
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

const FIELD_NAMES: Record<number, string> = {
  0: "AuthenticityProof",
  1: "DecryptionKey",
  2: "Expiry",
  3: "Channel",
  4: "Topic1",
  5: "Topic2",
  6: "Topic3",
  7: "Topic4",
  8: "Data",
};

const PROOF_NAMES: Record<number, string> = {
  0: "Sr25519",
  1: "Ed25519",
  2: "Secp256k1Ecdsa",
  3: "OnChain",
};

const NONCE_BYTES = 12;
const TAG_BYTES = 16;

/** AEAD output: `nonce12 ‖ ciphertext ‖ tag16`, split by fixed widths. */
function sealed(walk: Walk, out: Fields, path: string, length: number): void {
  if (length < NONCE_BYTES + TAG_BYTES)
    throw invalid("shorter than a nonce and a tag");
  out.nonce = walk.fixed(`${path}.nonce`, NONCE_BYTES);
  out.ciphertext = walk.fixed(
    `${path}.ciphertext`,
    length - NONCE_BYTES - TAG_BYTES,
  );
  out.tag = walk.fixed(`${path}.tag`, TAG_BYTES);
  walk.note(path, {
    layout: "ChaCha20-Poly1305: nonce 12 ‖ ciphertext ‖ tag 16",
  });
}

/** A request statement's data: `ChatRequestEncryptedRemote`. */
function requestData(walk: Walk, out: Fields): void {
  walk.vec(out, "encryptionPubKey", "$.encryptionPubKey");
  walk.note("$.encryptionPubKey", {
    meaning: "the sender's ephemeral X25519 public key",
  });
  const node: Fields = {
    length: walk.compact("$.encryptedRequest.length"),
  };
  out.encryptedRequest = node;
  sealed(walk, node, "$.encryptedRequest", node.length as number);
}

function sessionData(walk: Walk, out: Fields, length: number): void {
  sealed(walk, out, "$", length);
}

function statementField(
  walk: Walk,
  field: Fields,
  path: string,
  stream: "requests" | "sessions",
): void {
  const tag = walk.index(field, path, FIELD_NAMES);
  switch (tag) {
    case 0: {
      const proof: Fields = {};
      field.proof = proof;
      const kind = walk.index(proof, `${path}.proof`, PROOF_NAMES);
      if (kind !== 0)
        throw notRead(`proof index ${kind} is not read by this host`);
      proof.signature = walk.fixed(`${path}.proof.signature`, 64);
      proof.signer = walk.fixed(`${path}.proof.signer`, 32);
      return;
    }
    case 1:
      field.decryptionKey = walk.fixed(`${path}.decryptionKey`, 32);
      return;
    case 2: {
      const expiry = walk.u64(`${path}.expiry`);
      field.expiry = expiry;
      const value = BigInt(expiry);
      const upper = Number(value >> 32n);
      walk.note(`${path}.expiry`, {
        upper32: upper,
        lower32: Number(value & 0xffff_ffffn),
        reads:
          upper === 0xffff_ffff
            ? "upper 32 bits all set: never lapses; lower 32 bits: priority, seconds since the protocol epoch"
            : "upper 32 bits: expiry; lower 32 bits: priority",
      });
      return;
    }
    case 3:
      field.channel = walk.fixed(`${path}.channel`, 32);
      return;
    case 4:
    case 5:
    case 6:
    case 7:
      field.topic = walk.fixed(`${path}.topic`, 32);
      return;
    case 8: {
      const data: Fields = { length: walk.compact(`${path}.data.length`) };
      field.data = data;
      const length = data.length as number;
      const bytes = walk.take(`${path}.data`, length);
      walk.nested(data, `${path}.data`, bytes, (inner, out) =>
        stream === "requests"
          ? requestData(inner, out)
          : sessionData(inner, out, length),
      );
      walk.note(`${path}.data`, {
        schema:
          stream === "requests"
            ? "ChatRequestEncryptedRemote"
            : "sealed session data",
      });
      return;
    }
    default:
      throw invalid(`field index ${tag} is not in the statement schema`);
  }
}

/** An encoded signed statement: `Vec<Field>`. */
export function decodeStatementLayer(
  raw: Uint8Array,
  stream: "requests" | "sessions",
): Decoded {
  const decoded = decode(raw, (walk, out) => {
    const fields: Fields = { count: walk.compact("$.fields.count") };
    const items: Json[] = [];
    out.fields = fields;
    fields.items = items;
    for (let index = 0; index < (fields.count as number); index++) {
      const field: Fields = {};
      items.push(field);
      statementField(walk, field, `$.fields.items[${index}]`, stream);
    }
  });
  decoded.annotations.$ = {
    hash: hex(statementHash(raw)),
    schema: "sp_statement_store::Statement",
    ...decoded.annotations.$,
  };
  return decoded;
}

// ---------------------------------------------------------------------------
// Chat messages
// ---------------------------------------------------------------------------

/** `ChatMessageStatementContent` indexes, by the app's names. */
const CONTENT_NAMES: Record<number, string> = {
  0: "Text",
  1: "Token",
  3: "ContactAdded",
  4: "Reacted",
  5: "ReactionRemoved",
  7: "Reply",
  8: "DataChannelOffer",
  9: "DataChannelAnswer",
  10: "DataChannelIceCandidate",
  11: "DataChannelClosed",
  12: "Edited",
  13: "LeftChat",
  14: "ChatAccepted",
  15: "RichText",
  16: "CoinagePayment",
  17: "DeviceAdded",
  18: "DeviceRemoved",
  19: "CompactionCommit",
  20: "DeviceChatAccepted",
};

const PLATFORM_NAMES: Record<number, string> = {
  0: "ANDROID",
  1: "IOS",
  2: "IOS_VOIP",
};

const SENSITIVE_TOKEN =
  "a push token: it addresses this device's notifications";
const PUBLIC_KEYS = "public keys the peer announces for a device";

/** A millisecond timestamp as ISO text, when it is one. */
function isoTime(ms: string): string | null {
  const value = BigInt(ms);
  return value > 0n && value < 8_640_000_000_000_000n
    ? new Date(Number(value)).toISOString()
    : null;
}

function timestamp(walk: Walk, out: Fields, key: string, path: string): void {
  out[key] = walk.u64(path);
  walk.note(path, { iso: isoTime(out[key] as string) });
}

/** `TokenContent`: the token's bytes and the `TokenPlatform` index. */
function token(walk: Walk, out: Fields, path: string): void {
  walk.note(path, { sensitive: SENSITIVE_TOKEN });
  walk.vec(out, "token", `${path}.token`);
  const platform = walk.u8(`${path}.platform`);
  out.platform = platform;
  const name = PLATFORM_NAMES[platform];
  if (name === undefined)
    throw invalid(`platform index ${platform} is not in TokenPlatform`);
  walk.note(`${path}.platform`, { name });
}

const TICKET_ZEROED =
  "the ticket opens the file: this host replaces it with zeros before keeping the plaintext";

/** `FileVariant::P2PMixnet`: the root entry, the ticket, the node, and `FileMeta`. */
function attachment(walk: Walk, out: Fields, path: string): void {
  const variant = walk.index(out, path, { 0: "P2PMixnet" });
  if (variant !== 0)
    throw notRead(`file variant ${variant} is not read by this host`);
  walk.vec(out, "identifier", `${path}.identifier`);
  walk.vec(out, "claimTicket", `${path}.claimTicket`);
  walk.note(`${path}.claimTicket`, { sensitive: TICKET_ZEROED });
  const node: Fields = {};
  out.nodeEndpoint = node;
  const endpoint = walk.index(node, `${path}.nodeEndpoint`, { 0: "WssUrl" });
  if (endpoint !== 0)
    throw notRead(`node endpoint ${endpoint} is not read by this host`);
  walk.string(node, "url", `${path}.nodeEndpoint.url`);
  const meta: Fields = {};
  out.meta = meta;
  const kind = walk.index(meta, `${path}.meta`, {
    0: "General",
    1: "Image",
    2: "Video",
  });
  if (kind > 2) throw notRead(`file meta ${kind} is not read by this host`);
  walk.string(meta, "mimeType", `${path}.meta.mimeType`);
  meta.fileSize = walk.u32(`${path}.meta.fileSize`);
  if (kind === 1) {
    meta.width = walk.u32(`${path}.meta.width`);
    meta.height = walk.u32(`${path}.meta.height`);
  }
  if (kind === 2) meta.duration = walk.u32(`${path}.meta.duration`);
  if (kind !== 0)
    walk.option(
      meta,
      "thumbnail",
      `${path}.meta.thumbnail`,
      (value, valuePath) => walk.vec(value, "bytes", valuePath),
    );
}

/** `RichTextContent`: optional text, then optional attachments. */
function richText(walk: Walk, out: Fields, path: string): void {
  walk.option(out, "text", `${path}.text`, (value, valuePath) =>
    walk.stringInto(value, valuePath),
  );
  walk.option(out, "attachments", `${path}.attachments`, (value, valuePath) => {
    const count = walk.compact(`${valuePath}.count`);
    value.count = count;
    const items: Fields[] = [];
    value.items = items;
    for (let index = 0; index < count; index += 1) {
      const item: Fields = {};
      items.push(item);
      attachment(walk, item, `${valuePath}.items[${index}]`);
    }
  });
}

function content(walk: Walk, out: Fields, path: string): void {
  const index = walk.index(out, path, CONTENT_NAMES);
  switch (index) {
    case 0:
      walk.string(out, "text", `${path}.text`);
      return;
    case 1:
      token(walk, out, path);
      return;
    case 3:
    case 13:
      return;
    case 4:
    case 5:
      walk.string(out, "messageId", `${path}.messageId`);
      walk.string(out, "emoji", `${path}.emoji`);
      return;
    case 7:
      walk.string(out, "messageId", `${path}.messageId`);
      out.ownContent = {};
      richText(walk, out.ownContent as Fields, `${path}.ownContent`);
      return;
    case 8: {
      walk.vec(out, "sdp", `${path}.sdp`);
      const purpose: Fields = {};
      out.purpose = purpose;
      walk.index(purpose, `${path}.purpose`, { 0: "Audio", 1: "Video" });
      return;
    }
    case 9:
    case 10:
      walk.string(out, "offerMessageId", `${path}.offerMessageId`);
      walk.vec(out, "sdp", `${path}.sdp`);
      return;
    case 11:
      walk.string(out, "offerMessageId", `${path}.offerMessageId`);
      return;
    case 12:
      walk.string(out, "messageId", `${path}.messageId`);
      out.newContent = {};
      richText(walk, out.newContent as Fields, `${path}.newContent`);
      return;
    case 14:
      walk.string(out, "requestId", `${path}.requestId`);
      return;
    case 15:
      richText(walk, out, path);
      return;
    case 17:
      walk.vec(out, "statementAccountId", `${path}.statementAccountId`);
      walk.vec(out, "encryptionPublicKey", `${path}.encryptionPublicKey`);
      walk.note(path, { public: PUBLIC_KEYS });
      return;
    case 18:
      walk.vec(out, "statementAccountId", `${path}.statementAccountId`);
      return;
    case 20: {
      walk.string(out, "requestId", `${path}.requestId`);
      const device: Fields = {};
      out.device = device;
      device.statementAccountId = walk.fixed(
        `${path}.device.statementAccountId`,
        32,
      );
      device.encryptionPublicKey = walk.fixed(
        `${path}.device.encryptionPublicKey`,
        32,
      );
      walk.note(`${path}.device`, { public: PUBLIC_KEYS });
      return;
    }
    default:
      throw notRead(`content index ${index} is not read by this host`);
  }
}

/** `ChatMessageStatement`: id, timestamp, and the versioned content. */
function message(walk: Walk, out: Fields): void {
  walk.string(out, "id", "$.id");
  timestamp(walk, out, "timestamp", "$.timestamp");
  const versioned: Fields = {};
  out.versioned = versioned;
  const version = walk.index(versioned, "$.versioned", { 0: "V1" });
  if (version !== 0)
    throw notRead(`versioned index ${version} is not read by this host`);
  const body: Fields = {};
  versioned.content = body;
  content(walk, body, "$.versioned.content");
}

/** One encoded chat message, as a session request's list holds it. */
export const decodeMessageLayer = (encoded: Uint8Array): Decoded => {
  const decoded = decode(encoded, message);
  decoded.annotations.$ = {
    schema: "ChatMessageStatement",
    ...decoded.annotations.$,
  };
  return decoded;
};

// ---------------------------------------------------------------------------
// Decrypted statement data
// ---------------------------------------------------------------------------

const SESSION_NAMES: Record<number, string> = {
  0: "Request",
  1: "Response",
  2: "MultiRequest",
  3: "MultiResponse",
};

/** `StructuredStatementData`. Multi-device envelopes are not opened. */
function session(walk: Walk, out: Fields): void {
  const index = walk.index(out, "$", SESSION_NAMES);
  switch (index) {
    case 0: {
      walk.string(out, "requestId", "$.requestId");
      const messages: Fields = { count: walk.compact("$.messages.count") };
      const items: Json[] = [];
      out.messages = messages;
      messages.items = items;
      for (let item = 0; item < (messages.count as number); item++) {
        const path = `$.messages.items[${item}]`;
        const offset = walk.reader.offset;
        // A Vec<u8> whose bytes are one encoded message, read in their place.
        const node: Fields = { length: walk.compact(`${path}.length`) };
        items.push(node);
        const bytes = walk.take(`${path}.message`, node.length as number);
        const encoded: Fields = {};
        node.message = encoded;
        walk.note(path, { offset, bytes: hex(bytes) });
        walk.nested(encoded, `${path}.message`, bytes, message);
        walk.note(`${path}.message`, { schema: "ChatMessageStatement" });
      }
      return;
    }
    case 1: {
      walk.string(out, "requestId", "$.requestId");
      out.responseCode = walk.u8("$.responseCode");
      if (out.responseCode === ACKNOWLEDGED)
        walk.note("$.responseCode", { name: "acknowledged" });
      return;
    }
    case 2:
    case 3:
      throw notRead("multi-device envelopes are not opened by this host");
    default:
      throw invalid(`index ${index} is not in StructuredStatementData`);
  }
}

const REQUEST_NAMES: Record<number, string> = { 0: "V1", 1: "V2" };

/** `ChatRequestDecrypted`: the request message, then the requester's proof. */
function request(walk: Walk, out: Fields): void {
  const body: Fields = {};
  out.message = body;
  walk.string(body, "messageId", "$.message.messageId");
  timestamp(walk, body, "timestamp", "$.message.timestamp");
  const versioned: Fields = {};
  body.content = versioned;
  const path = "$.message.content";
  const version = walk.index(versioned, path, REQUEST_NAMES);
  if (version === 1) {
    const proof: Fields = {};
    versioned.identityProof = proof;
    proof.identityAccountId = walk.fixed(
      `${path}.identityProof.identityAccountId`,
      32,
    );
    proof.proof = walk.fixed(`${path}.identityProof.proof`, 32);
    versioned.deviceEncPubKey = walk.fixed(`${path}.deviceEncPubKey`, 32);
  } else if (version !== 0)
    throw invalid(`index ${version} is not in VersionedRequestContent`);
  walk.option(versioned, "pushToken", `${path}.pushToken`, (value, valuePath) =>
    token(walk, value, valuePath),
  );
  walk.option(
    versioned,
    "welcomeMessage",
    `${path}.welcomeMessage`,
    (value, valuePath) => richText(walk, value, valuePath),
  );
  walk.note("$.message", { signedBytes: walk.reader.offset });
  const proof: Fields = {};
  out.proof = proof;
  const kind = walk.index(proof, "$.proof", { 0: "Sr25519" });
  if (kind !== 0) throw notRead(`proof index ${kind} is not read by this host`);
  proof.signature = walk.fixed("$.proof.signature", 64);
  proof.signer = walk.fixed("$.proof.signer", 32);
}

/** Decrypted statement data, read as its stream lays it out. */
export function decodePlaintextLayer(
  plaintext: Uint8Array,
  stream: "requests" | "sessions",
): Decoded {
  const decoded = decode(plaintext, stream === "requests" ? request : session);
  decoded.annotations.$ = {
    schema:
      stream === "requests"
        ? "ChatRequestDecrypted"
        : "StructuredStatementData",
    ...decoded.annotations.$,
  };
  return decoded;
}
