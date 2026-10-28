import type { HexString } from "@parity/truapi/scale";
import { CONTROL_TAGS } from "./protocol.js";

/**
 * What the host keeps of its chats, per wallet and network, in this
 * browser's `localStorage`. Nothing here is secret: keys are derived again
 * from the wallet on every start. The record is the host's own and is not
 * synced anywhere, so another device of the same identity does not see it.
 */

/**
 * `left`: one side left the conversation (a system line, kind 13). `call`:
 * a call one side offered (kind 8), with what became of it.
 */
export type MessageKind =
  | "text"
  | "request"
  | "accepted"
  | "left"
  | "call"
  | "unsupported";

export type MessageStatus =
  | "pending"
  | "sent"
  | "acknowledged"
  | "failed"
  | "received";

export interface StoredMessage {
  id: string;
  direction: "in" | "out";
  kind: MessageKind;
  text: string;
  /** Milliseconds since the epoch, as the sender stamped it. */
  at: number;
  status: MessageStatus;
  /** Why a send failed, when it did. */
  problem?: string;
  /** The emoji on this message, one entry per emoji and side. */
  reactions?: StoredReaction[];
  /** Files the message carries; `text` is then their caption, possibly empty. */
  attachments?: StoredAttachment[];
  /** For a `call` message: the call it offered, whose id is the message's. */
  call?: StoredCall;
}

export interface StoredCall {
  video: boolean;
  /** The offer's minimal setup, as 0x hex, kept until the peer acknowledges it. */
  setup?: HexString;
  /** When the other side answered. */
  answeredAt?: number;
  /** Who ended the call, and when. */
  closedBy?: "self" | "peer";
  endedAt?: number;
}

/**
 * A file a message carries, sent through HOP. The ticket opens the file, so
 * it is kept with the message, as the message's text is, and never shown
 * or reported.
 */
export interface StoredAttachment {
  /** The root entry's hash, as 0x hex. Absent while this host is still uploading it. */
  identifier?: HexString;
  /** As 0x hex. Absent while this host is still uploading it. */
  ticket?: HexString;
  /** The HOP node the file was given to. */
  node?: string;
  kind: "file" | "image" | "video";
  mimeType: string;
  /** As the sender gave it. */
  size: number;
  width?: number;
  height?: number;
  duration?: number;
  /** The file's name, for files this host sent: the wire does not carry one. */
  name?: string;
}

export interface StoredReaction {
  emoji: string;
  from: "self" | "peer";
}

/**
 * A message this host sends that is not a bubble, such as a reaction. It is
 * kept until the peer acknowledges it, and sent again with every request
 * until then.
 */
export interface PendingMessage {
  id: string;
  at: number;
  content:
    | { kind: "reaction"; messageId: string; emoji: string; removed: boolean }
    /** A call's answer or candidates, with their bytes as 0x hex, or its end. */
    | { kind: "call-answer"; offerId: string; sdp: HexString }
    | { kind: "call-candidates"; offerId: string; sdp: HexString }
    | { kind: "call-closed"; offerId: string };
}

export interface Contact {
  account: HexString;
  chatKey: HexString;
  name: string | null;
  /** `requested`: this host sent a request and no answer came yet. */
  state: "requested" | "active";
  messages: StoredMessage[];
  /** The last request this host put on its request channel, and the messages in it. */
  outbound: { requestId: string; messageIds: string[] } | null;
  /** The last expiry used on each of this host's channels, as decimal text. */
  expiry: { request: string | null; response: string | null };
  /** The peer's last request this host acknowledged. */
  acknowledged: string | null;
  /**
   * The user blocked this person here. Nothing is sent to them about it:
   * the chat leaves the list, its session is not followed, and nothing they
   * send is read or acknowledged until the user unblocks them.
   */
  blocked?: boolean;
  /** The peer said they left the chat (kind 13), and has not come back. */
  peerLeft?: boolean;
  /** Reactions and call signals sent and not acknowledged yet. */
  pending?: PendingMessage[];
  /**
   * Ids of the peer's reactions and call signals already handled, newest
   * last, so a delivery seen again changes nothing.
   */
  seenSignals?: string[];
}

export interface IncomingRequest {
  id: string;
  peer: HexString;
  peerChatKey: HexString | null;
  name: string | null;
  welcome: string | null;
  at: number;
  version: 1 | 2;
  /** Why the request cannot be accepted, when it cannot. */
  problem: string | null;
}

export interface ChatRecord {
  contacts: Contact[];
  requests: IncomingRequest[];
  /** Request ids this host declined, so a stored request does not come back. */
  declined: string[];
  /**
   * Conversations the user left, kept as they were and no longer shown or
   * followed. A new request from the same person starts a new conversation.
   */
  left: Contact[];
}

const empty = (): ChatRecord => ({
  contacts: [],
  requests: [],
  declined: [],
  left: [],
});

/** An account id shortened for a name line, when the account has no name. */
export const abbreviateAccount = (account: string): string =>
  `${account.slice(0, 8)}…${account.slice(-6)}`;

export const chatRecordKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:chat:${walletId}:v1`;

const PLACEHOLDER_TAG =
  /^A message this host does not show yet \(kind (\d+)\)$/;

/**
 * Drop the placeholders an earlier version stored for control messages (a
 * push token, a device change), which carry nothing to show. Placeholders of
 * other kinds stay.
 */
function withoutControlPlaceholders(contact: Contact): Contact {
  return {
    ...contact,
    messages: contact.messages.filter((message) => {
      const tag = PLACEHOLDER_TAG.exec(message.text)?.[1];
      return !(
        message.kind === "unsupported" &&
        tag !== undefined &&
        CONTROL_TAGS.includes(Number(tag))
      );
    }),
  };
}

/** Load a record. One this cannot read is set aside under a `.unreadable` key, never lost. */
export function loadChatRecord(storage: Storage, key: string): ChatRecord {
  const raw = storage.getItem(key);
  if (raw === null) return empty();
  try {
    const parsed = JSON.parse(raw) as Partial<ChatRecord>;
    if (!Array.isArray(parsed.contacts) || !Array.isArray(parsed.requests))
      throw new Error("not a chat record");
    return {
      contacts: parsed.contacts.map(withoutControlPlaceholders),
      requests: parsed.requests,
      declined: Array.isArray(parsed.declined) ? parsed.declined : [],
      left: Array.isArray(parsed.left) ? parsed.left : [],
    };
  } catch {
    storage.setItem(`${key}.unreadable`, raw);
    return empty();
  }
}

/** The accounts the record at `key` holds as blocked, read without writing anything. */
export function blockedAccounts(
  storage: Pick<Storage, "getItem">,
  key: string,
): HexString[] {
  const raw = storage.getItem(key);
  if (raw === null) return [];
  const readOnly = { getItem: () => raw, setItem: () => {} };
  return loadChatRecord(readOnly as unknown as Storage, key)
    .contacts.filter((contact) => contact.blocked === true)
    .map((contact) => contact.account);
}

export function saveChatRecord(
  storage: Storage,
  key: string,
  record: ChatRecord,
): void {
  storage.setItem(key, JSON.stringify(record));
}
