import {
  Reader,
  bytes,
  concat,
  list,
  option,
  readBytes,
  readOption,
  text,
  u64,
} from "../../platform/codec.js";

/**
 * PROTOTYPE. SCALE layouts of the Nova chat wire, read from the official apps:
 * Android `ChatRequestRemoteModel.kt`, `ChatMessageStatementContent.kt`,
 * `StructuredStatementData.kt`, `MultiDeviceModels.kt`, `DeviceInfoScale.kt`
 * at fc186861db; iOS `ChatRequestFactory.swift`, `MultiDeviceModels.swift` at
 * 22407b1058.
 */

export type Bytes = Uint8Array;

// ---- Chat messages -----------------------------------------------------------

export interface DeviceInfo {
  statementAccount: Bytes;
  encryptionKey: Bytes;
}

/** Content the worker may send and receive. */
export type UserContent =
  | { kind: "Text"; text: string }
  | { kind: "RichText"; text: string | null }
  | { kind: "Reacted"; messageId: string; emoji: string }
  | { kind: "ReactionRemoved"; messageId: string; emoji: string }
  | { kind: "Reply"; messageId: string; text: string | null }
  | { kind: "Edited"; messageId: string; text: string | null }
  | { kind: "LeftChat" };

export type Content =
  | UserContent
  | { kind: "DeviceChatAccepted"; requestId: string; device: DeviceInfo }
  /** A known or unknown kind this MVP does not handle (calls, files, payments, device changes, Seen, ...). */
  | { kind: "Unsupported"; index: number };

export interface ChatMessage {
  id: string;
  timestamp: bigint;
  content: Content;
}

const INDEX = {
  Text: 0,
  Reacted: 4,
  ReactionRemoved: 5,
  Reply: 7,
  Edited: 12,
  LeftChat: 13,
  RichText: 15,
  DeviceChatAccepted: 20,
} as const;

/** RichTextContent `{ text: Option<String>, attachments: Option<Vec<FileVariant>> }`, text only. */
const richText = (value: string | null): Bytes =>
  concat(option(value, text), Uint8Array.of(0));

/** Reads a RichTextContent; null when it carries attachments, which the MVP does not handle. */
function readRichText(reader: Reader): { text: string | null } | null {
  const value = readOption(reader, (r) => r.text());
  if (reader.u8() !== 0) return null;
  return { text: value };
}

function encodeContent(content: Content): Bytes {
  switch (content.kind) {
    case "Text":
      return concat(Uint8Array.of(INDEX.Text), text(content.text));
    case "RichText":
      return concat(Uint8Array.of(INDEX.RichText), richText(content.text));
    case "Reacted":
    case "ReactionRemoved":
      return concat(
        Uint8Array.of(INDEX[content.kind]),
        text(content.messageId),
        text(content.emoji),
      );
    case "Reply":
    case "Edited":
      return concat(
        Uint8Array.of(INDEX[content.kind]),
        text(content.messageId),
        richText(content.text),
      );
    case "LeftChat":
      return Uint8Array.of(INDEX.LeftChat);
    case "DeviceChatAccepted":
      if (
        content.device.statementAccount.length !== 32 ||
        content.device.encryptionKey.length !== 32
      )
        throw new Error("device fields are 32 bytes");
      return concat(
        Uint8Array.of(INDEX.DeviceChatAccepted),
        text(content.requestId),
        content.device.statementAccount,
        content.device.encryptionKey,
      );
    case "Unsupported":
      throw new Error("an unsupported kind is never sent");
  }
}

/** `ChatMessageStatement { id: String, timestamp: u64, versioned: V1 { content } }`. */
export const encodeMessage = (message: ChatMessage): Bytes =>
  concat(
    text(message.id),
    u64(message.timestamp),
    Uint8Array.of(0),
    encodeContent(message.content),
  );

/**
 * Decodes a chat message. Trailing bytes are ignored, as Android does. A kind
 * this MVP does not handle, a RichText with attachments, or a newer message
 * version becomes `Unsupported`; only the id and timestamp must read.
 */
export function decodeMessage(encoded: Bytes): ChatMessage {
  const reader = new Reader(encoded);
  const id = reader.text();
  const timestamp = reader.u64();
  const unsupported = (index: number): ChatMessage => ({
    id,
    timestamp,
    content: { kind: "Unsupported", index },
  });
  if (reader.u8() !== 0) return unsupported(-1);
  const index = reader.u8();
  try {
    switch (index) {
      case INDEX.Text:
        return {
          id,
          timestamp,
          content: { kind: "Text", text: reader.text() },
        };
      case INDEX.RichText: {
        const rich = readRichText(reader);
        return rich
          ? { id, timestamp, content: { kind: "RichText", text: rich.text } }
          : unsupported(index);
      }
      case INDEX.Reacted:
      case INDEX.ReactionRemoved:
        return {
          id,
          timestamp,
          content: {
            kind: index === INDEX.Reacted ? "Reacted" : "ReactionRemoved",
            messageId: reader.text(),
            emoji: reader.text(),
          },
        };
      case INDEX.Reply:
      case INDEX.Edited: {
        const messageId = reader.text();
        const rich = readRichText(reader);
        if (!rich) return unsupported(index);
        return {
          id,
          timestamp,
          content: {
            kind: index === INDEX.Reply ? "Reply" : "Edited",
            messageId,
            text: rich.text,
          },
        };
      }
      case INDEX.LeftChat:
        return { id, timestamp, content: { kind: "LeftChat" } };
      case INDEX.DeviceChatAccepted:
        return {
          id,
          timestamp,
          content: {
            kind: "DeviceChatAccepted",
            requestId: reader.text(),
            device: {
              statementAccount: reader.take(32).slice(),
              encryptionKey: reader.take(32).slice(),
            },
          },
        };
      default:
        return unsupported(index);
    }
  } catch {
    return unsupported(index);
  }
}

export const USER_KINDS: ReadonlySet<Content["kind"]> = new Set([
  "Text",
  "RichText",
  "Reacted",
  "ReactionRemoved",
  "Reply",
  "Edited",
  "LeftChat",
]);

export const isUserContent = (content: Content): content is UserContent =>
  USER_KINDS.has(content.kind);

// ---- Transport frames (StructuredStatementData) --------------------------------

export type Frame =
  | { kind: "Request"; requestId: string; messages: Bytes[] }
  | { kind: "Response"; requestId: string; code: number };

export interface DeviceEntry {
  statementAccount: Bytes;
  encryptedKey: Bytes;
}

export type StatementData =
  | Frame
  | {
      kind: "MultiRequest" | "MultiResponse";
      payload: Bytes;
      devices: DeviceEntry[];
    };

/** A frame without its variant byte: the inner payload of a multi-device envelope. */
export const encodeFrameBody = (frame: Frame): Bytes =>
  frame.kind === "Request"
    ? concat(text(frame.requestId), list(frame.messages, bytes))
    : concat(text(frame.requestId), Uint8Array.of(frame.code));

export function decodeFrameBody(kind: Frame["kind"], encoded: Bytes): Frame {
  const reader = new Reader(encoded);
  const requestId = reader.text();
  const frame: Frame =
    kind === "Request"
      ? {
          kind,
          requestId,
          messages: Array.from({ length: reader.compact() }, () =>
            readBytes(reader),
          ),
        }
      : { kind, requestId, code: reader.u8() };
  reader.finish();
  return frame;
}

export function encodeStatementData(data: StatementData): Bytes {
  switch (data.kind) {
    case "Request":
      return concat(Uint8Array.of(0), encodeFrameBody(data));
    case "Response":
      return concat(Uint8Array.of(1), encodeFrameBody(data));
    case "MultiRequest":
    case "MultiResponse":
      return concat(
        Uint8Array.of(data.kind === "MultiRequest" ? 2 : 3),
        bytes(data.payload),
        list(data.devices, (device) => {
          if (device.statementAccount.length !== 32)
            throw new Error("a device account is 32 bytes");
          return concat(device.statementAccount, bytes(device.encryptedKey));
        }),
      );
  }
}

export function decodeStatementData(encoded: Bytes): StatementData {
  const reader = new Reader(encoded);
  const tag = reader.u8();
  let data: StatementData;
  if (tag === 0 || tag === 1) {
    const requestId = reader.text();
    data =
      tag === 0
        ? {
            kind: "Request",
            requestId,
            messages: Array.from({ length: reader.compact() }, () =>
              readBytes(reader),
            ),
          }
        : { kind: "Response", requestId, code: reader.u8() };
  } else if (tag === 2 || tag === 3) {
    const payload = readBytes(reader);
    const devices = Array.from({ length: reader.compact() }, () => ({
      statementAccount: reader.take(32).slice(),
      encryptedKey: readBytes(reader),
    }));
    data = {
      kind: tag === 2 ? "MultiRequest" : "MultiResponse",
      payload,
      devices,
    };
  } else throw new Error(`statement data tag ${tag}`);
  reader.finish();
  return data;
}

// ---- Chat requests ---------------------------------------------------------------

export interface RequestMessageV2 {
  messageId: string;
  timestamp: bigint;
  identityAccount: Bytes;
  identityProof: Bytes;
  deviceEncryptionKey: Bytes;
  welcome: string | null;
}

/** `ChatRequestMessage { messageId, timestamp, content: V2 { identityProof { identity, proof }, deviceEncPubKey, pushToken: None, welcome } }`. */
export const encodeRequestMessage = (message: RequestMessageV2): Bytes =>
  concat(
    text(message.messageId),
    u64(message.timestamp),
    Uint8Array.of(1),
    message.identityAccount,
    message.identityProof,
    message.deviceEncryptionKey,
    Uint8Array.of(0),
    option(message.welcome, richText),
  );

/** Reads a request message. Throws on V1 and on any version other than V2. A push token is read and dropped. */
export function readRequestMessage(reader: Reader): RequestMessageV2 {
  const messageId = reader.text();
  const timestamp = reader.u64();
  const version = reader.u8();
  if (version !== 1)
    throw new Error(`request content version ${version} is not supported`);
  const identityAccount = reader.take(32).slice();
  const identityProof = reader.take(32).slice();
  const deviceEncryptionKey = reader.take(32).slice();
  readOption(reader, (r) => ({ token: readBytes(r), platform: r.u8() }));
  const welcome = readOption(reader, (r) => {
    const rich = readRichText(r);
    if (!rich) throw new Error("a welcome with attachments is not supported");
    return rich.text;
  });
  return {
    messageId,
    timestamp,
    identityAccount,
    identityProof,
    deviceEncryptionKey,
    welcome: welcome ?? null,
  };
}

/** `ChatRequestProofPayload { message, acceptor: Vec<u8> }`: what the inner sr25519 signature signs. */
export const requestProofPayload = (message: Bytes, acceptor: Bytes): Bytes =>
  concat(message, bytes(acceptor));

/** `ChatRequestDecrypted { message, proof: Sr25519 { signature, signer } }`. */
export const encodeRequestDecrypted = (
  message: Bytes,
  signature: Bytes,
  signer: Bytes,
): Bytes => concat(message, Uint8Array.of(0), signature, signer);

/** `ChatRequestEncryptedRemote { encryptionPubKey: Vec<u8>, encryptedRequest: Vec<u8> }`. */
export const encodeRequestEnvelope = (ephemeral: Bytes, sealed: Bytes): Bytes =>
  concat(bytes(ephemeral), bytes(sealed));

export function decodeRequestEnvelope(encoded: Bytes): {
  ephemeral: Bytes;
  sealed: Bytes;
} {
  const reader = new Reader(encoded);
  const ephemeral = readBytes(reader);
  const sealed = readBytes(reader);
  reader.finish();
  return { ephemeral, sealed };
}

/** `IdentityProofPayloadScale { identity: [u8; 32], statementAccount: [u8; 32], context: "mds-chat-request" }`. */
export const identityProofPayload = (
  identity: Bytes,
  statementAccount: Bytes,
): Bytes => concat(identity, statementAccount, text("mds-chat-request"));
