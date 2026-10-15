import {
  Reader,
  bytes,
  concat,
  equalBytes,
  list,
  option,
  readBytes,
  readList,
  readOption,
  text,
  u64,
  utf8,
} from "./codec.js";
import {
  aeadKey,
  blake2b256,
  keyedBlake2b256,
  seal,
  sr25519Verify,
  unseal,
  x25519Agree,
  x25519PublicKey,
  type RandomBytes,
} from "./crypto.js";

/**
 * The Polkadot app's one-to-one chat wire, written from the app's protocol
 * (Android `feature/chats` and `feature/statement-store`, iOS
 * `Modules/IncomingChatRequest` and `Modules/Chat`, vendored under `hosts/`).
 * No code is taken from them. Only the parts a host with one device needs are
 * here: first-contact requests, pairwise sessions, text and acceptance.
 * Multi-device envelopes are recognised and reported, not opened.
 */

const SLASH = utf8("/");

const key32 = (value: Uint8Array, what: string): Uint8Array => {
  if (value.length !== 32) throw new Error(`${what} must be 32 bytes`);
  return value;
};

// ---------------------------------------------------------------------------
// Topics and channels
// ---------------------------------------------------------------------------

/** The topic every request to `acceptor` carries. */
export const requestTopic = (acceptor: Uint8Array): Uint8Array =>
  blake2b256(concat(text("chat-request"), bytes(key32(acceptor, "acceptor"))));

/** The topic of requests to `acceptor` sent on request day `day`. */
export const requestDayTopic = (
  acceptor: Uint8Array,
  day: bigint,
): Uint8Array =>
  blake2b256(
    concat(text("chat-request"), bytes(key32(acceptor, "acceptor")), u64(day)),
  );

function pairTopic(
  prefix: string,
  secret: Uint8Array,
  from: Uint8Array,
  to: Uint8Array,
) {
  return keyedBlake2b256(
    key32(secret, "shared secret"),
    concat(
      utf8(prefix),
      key32(from, "sender"),
      key32(to, "receiver"),
      SLASH,
      SLASH,
    ),
  );
}

/** The third topic of a request: the pair's request session. */
export const requestSessionTopic = (
  secret: Uint8Array,
  requester: Uint8Array,
  acceptor: Uint8Array,
) => pairTopic("chat-request", secret, requester, acceptor);

/** The topic `sender` writes to `receiver` on, keyed by their agreed chat secret. */
export const sessionTopic = (
  secret: Uint8Array,
  sender: Uint8Array,
  receiver: Uint8Array,
) => pairTopic("session", secret, sender, receiver);

export const requestChannel = (topic: Uint8Array): Uint8Array =>
  keyedBlake2b256(topic, utf8("request"));
export const responseChannel = (topic: Uint8Array): Uint8Array =>
  keyedBlake2b256(topic, utf8("response"));

// ---------------------------------------------------------------------------
// Chat requests
// ---------------------------------------------------------------------------

/** The text a request opens with. Attachments are not read. */
export type RequestContent =
  | { version: 1; welcome: string | null }
  | {
      version: 2;
      identity: Uint8Array;
      identityProof: Uint8Array;
      deviceKey: Uint8Array;
      welcome: string | null;
    };

export interface ChatRequest {
  id: string;
  timestampMs: bigint;
  content: RequestContent;
}

/** A request opened and its signature checked. */
export interface OpenedRequest extends ChatRequest {
  /** The key that signed it: the identity for version 1, the sending device for version 2. */
  signer: Uint8Array;
  /** The account the conversation is with: the signer for version 1, the named identity for version 2. */
  peer: Uint8Array;
}

/** `RichTextContent`: optional text, then optional attachments, which this host never sends. */
const encodeRichText = (value: string): Uint8Array =>
  concat(option(value, text), Uint8Array.of(0));

function readRichText(reader: Reader): {
  text: string | null;
  attachments: boolean;
} {
  const body = readOption(reader, (inner) => inner.text());
  const attachments = readOption(reader, (inner) => {
    // An attachment list cannot be skipped without its layout; stop here.
    if (inner.compact() > 0) throw new UnsupportedContent("attachments");
    return true;
  });
  return { text: body, attachments: attachments !== null };
}

/** A token: a push token and its platform byte. Read only to be skipped. */
function skipToken(reader: Reader): void {
  readBytes(reader);
  reader.u8();
}

export function encodeRequestMessage(request: ChatRequest): Uint8Array {
  if (request.content.version !== 1)
    throw new Error("this host sends version 1 requests only");
  return concat(
    text(request.id),
    u64(request.timestampMs),
    Uint8Array.of(0),
    Uint8Array.of(0),
    option(request.content.welcome, encodeRichText),
  );
}

function readRequestMessage(reader: Reader): ChatRequest {
  const id = reader.text();
  const timestampMs = reader.u64();
  const version = reader.u8();
  if (version === 0) {
    readOption(reader, skipToken);
    const welcome = readOption(reader, readRichText)?.text ?? null;
    return { id, timestampMs, content: { version: 1, welcome } };
  }
  if (version === 1) {
    const identity = reader.take(32).slice();
    const identityProof = reader.take(32).slice();
    const deviceKey = reader.take(32).slice();
    readOption(reader, skipToken);
    const welcome = readOption(reader, readRichText)?.text ?? null;
    return {
      id,
      timestampMs,
      content: { version: 2, identity, identityProof, deviceKey, welcome },
    };
  }
  throw new Error(`the request is version tag ${version}`);
}

/** What the request's signer signs: the message, then the acceptor as `Vec<u8>`. */
const requestSigningPayload = (message: Uint8Array, acceptor: Uint8Array) =>
  concat(message, bytes(acceptor));

/**
 * Seal a version 1 request to `acceptorChatKey` with a fresh ephemeral key:
 * `Vec<u8> ephemeral || Vec<u8> sealed(message || 0x00 || signature || signer)`.
 */
export function sealRequest(
  request: ChatRequest,
  acceptor: Uint8Array,
  acceptorChatKey: Uint8Array,
  signer: Uint8Array,
  sign: (payload: Uint8Array) => Uint8Array,
  random: RandomBytes,
): Uint8Array {
  const message = encodeRequestMessage(request);
  const signature = sign(requestSigningPayload(message, acceptor));
  const plaintext = concat(message, Uint8Array.of(0), signature, signer);
  const ephemeral = random(32);
  const key = aeadKey(x25519Agree(ephemeral, acceptorChatKey));
  return concat(
    bytes(x25519PublicKey(ephemeral)),
    bytes(seal(key, plaintext, random)),
  );
}

/**
 * Open a request sent to `acceptor` and check its signature. The identity
 * proof of a version 2 request needs the peer's chat key, so it is checked
 * apart, with {@link checkIdentityProof}.
 */
export function openRequest(
  data: Uint8Array,
  acceptor: Uint8Array,
  agree: (ephemeral: Uint8Array) => Uint8Array,
): OpenedRequest {
  const outer = new Reader(data);
  const ephemeral = key32(readBytes(outer), "the ephemeral key");
  const sealed = readBytes(outer);
  outer.finish();
  const plaintext = unseal(aeadKey(agree(ephemeral)), sealed);
  const reader = new Reader(plaintext);
  const request = readRequestMessage(reader);
  const messageEnd = plaintext.length - 97;
  const proofKind = reader.u8();
  if (proofKind !== 0)
    throw new Error(`the request proof is kind ${proofKind}, not sr25519`);
  const signature = reader.take(64).slice();
  const signer = reader.take(32).slice();
  reader.finish();
  const message = plaintext.subarray(0, messageEnd);
  if (
    !sr25519Verify(requestSigningPayload(message, acceptor), signature, signer)
  )
    throw new Error("the request signature does not verify");
  const peer =
    request.content.version === 2 ? request.content.identity : signer;
  return { ...request, signer, peer };
}

/**
 * Check a version 2 request's identity proof:
 * `blake2b(key: X25519(own chat, peer chat), identity || signer || "mds-chat-request")`.
 */
export function checkIdentityProof(
  request: OpenedRequest,
  secret: Uint8Array,
): boolean {
  if (request.content.version !== 2) return true;
  const expected = keyedBlake2b256(
    secret,
    concat(request.content.identity, request.signer, text("mds-chat-request")),
  );
  return equalBytes(expected, request.content.identityProof);
}

// ---------------------------------------------------------------------------
// Session messages
// ---------------------------------------------------------------------------

/**
 * Messages that update a peer's state instead of the conversation: a push
 * token (kind 1) and the peer's device list (kinds 17 and 18). The apps show
 * none of them in a chat.
 */
export type ControlContent =
  | { control: "token" }
  | {
      control: "device-added";
      statementAccount: Uint8Array;
      encryptionKey: Uint8Array;
    }
  | { control: "device-removed"; statementAccount: Uint8Array };

/** A chat message's content. Kinds this host does not handle keep their tag. */
export type MessageContent =
  | { kind: "text"; text: string }
  | { kind: "accepted"; requestId: string }
  | ({ kind: "control" } & ControlContent)
  | { kind: "other"; tag: number };

export interface ChatMessage {
  id: string;
  timestampMs: bigint;
  content: MessageContent;
}

const CONTENT_TAG = {
  text: 0,
  token: 1,
  reply: 7,
  chatAccepted: 14,
  richText: 15,
  deviceAdded: 17,
  deviceRemoved: 18,
  deviceChatAccepted: 20,
} as const;

/** Content tags that carry control data, never a chat bubble. */
export const CONTROL_TAGS: readonly number[] = [
  CONTENT_TAG.token,
  CONTENT_TAG.deviceAdded,
  CONTENT_TAG.deviceRemoved,
];

/** The push platforms a token names: Android, iOS, iOS VoIP. */
const TOKEN_PLATFORMS = 3;

const readKey = (reader: Reader, what: string): Uint8Array =>
  key32(readBytes(reader), what);

class UnsupportedContent extends Error {}

/** Encode `id || timestamp || 0x00 (version 1) || content`. Sends text and plain acceptance. */
export function encodeMessage(message: ChatMessage): Uint8Array {
  const head = concat(
    text(message.id),
    u64(message.timestampMs),
    Uint8Array.of(0),
  );
  switch (message.content.kind) {
    case "text":
      return concat(
        head,
        Uint8Array.of(CONTENT_TAG.text),
        text(message.content.text),
      );
    case "accepted":
      return concat(
        head,
        Uint8Array.of(CONTENT_TAG.chatAccepted),
        text(message.content.requestId),
      );
    case "control":
    case "other":
      throw new Error("this host does not send that kind of message");
  }
}

/** Decode one message. A kind this host does not read decodes as `other` with its tag. */
export function decodeMessage(encoded: Uint8Array): ChatMessage {
  const reader = new Reader(encoded);
  const id = reader.text();
  const timestampMs = reader.u64();
  const version = reader.u8();
  if (version !== 0) throw new Error(`the message is version tag ${version}`);
  const tag = reader.u8();
  const content = readContent(tag, reader);
  if (content.kind !== "other") reader.finish();
  return { id, timestampMs, content };
}

function readContent(tag: number, reader: Reader): MessageContent {
  try {
    switch (tag) {
      case CONTENT_TAG.text:
        return { kind: "text", text: reader.text() };
      case CONTENT_TAG.richText: {
        const rich = readRichText(reader);
        return rich.text === null
          ? { kind: "other", tag }
          : { kind: "text", text: rich.text };
      }
      case CONTENT_TAG.reply: {
        reader.text();
        const rich = readRichText(reader);
        return rich.text === null
          ? { kind: "other", tag }
          : { kind: "text", text: rich.text };
      }
      case CONTENT_TAG.chatAccepted:
        return { kind: "accepted", requestId: reader.text() };
      case CONTENT_TAG.token: {
        readBytes(reader);
        const platform = reader.u8();
        if (platform >= TOKEN_PLATFORMS)
          throw new Error(`the token names platform ${platform}`);
        return { kind: "control", control: "token" };
      }
      case CONTENT_TAG.deviceAdded:
        return {
          kind: "control",
          control: "device-added",
          statementAccount: readKey(reader, "a device's statement account"),
          encryptionKey: readKey(reader, "a device's encryption key"),
        };
      case CONTENT_TAG.deviceRemoved:
        return {
          kind: "control",
          control: "device-removed",
          statementAccount: readKey(reader, "a device's statement account"),
        };
      case CONTENT_TAG.deviceChatAccepted: {
        const requestId = reader.text();
        reader.take(64);
        return { kind: "accepted", requestId };
      }
      default:
        return { kind: "other", tag };
    }
  } catch (error) {
    if (error instanceof UnsupportedContent) return { kind: "other", tag };
    throw error;
  }
}

/** What a session statement carries once opened. */
export type SessionData =
  | { kind: "request"; requestId: string; messages: Uint8Array[] }
  | { kind: "response"; requestId: string; code: number }
  | { kind: "multi-device" };

export function encodeSessionData(data: SessionData): Uint8Array {
  switch (data.kind) {
    case "request":
      return concat(
        Uint8Array.of(0),
        text(data.requestId),
        list(data.messages, bytes),
      );
    case "response":
      return concat(
        Uint8Array.of(1),
        text(data.requestId),
        Uint8Array.of(data.code),
      );
    case "multi-device":
      throw new Error("this host does not send multi-device envelopes");
  }
}

export function decodeSessionData(encoded: Uint8Array): SessionData {
  const reader = new Reader(encoded);
  const tag = reader.u8();
  if (tag === 2 || tag === 3) return { kind: "multi-device" };
  const requestId = reader.text();
  let data: SessionData;
  if (tag === 0)
    data = {
      kind: "request",
      requestId,
      messages: readList(reader, readBytes),
    };
  else if (tag === 1) data = { kind: "response", requestId, code: reader.u8() };
  else throw new Error(`the session data is tag ${tag}`);
  reader.finish();
  return data;
}

/** Seal session data under the pair's agreed secret. */
export const sealSession = (
  secret: Uint8Array,
  data: SessionData,
  random: RandomBytes,
) => seal(aeadKey(secret), encodeSessionData(data), random);

export const openSession = (
  secret: Uint8Array,
  sealed: Uint8Array,
): SessionData => decodeSessionData(unseal(aeadKey(secret), sealed));

/** The response code that acknowledges a request. */
export const ACKNOWLEDGED = 0;
