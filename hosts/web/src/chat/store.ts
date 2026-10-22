import type { HexString } from "@parity/truapi/scale";
import { CONTROL_TAGS } from "./protocol.js";

/**
 * What the host keeps of its chats, per wallet and network, in this
 * browser's `localStorage`. Nothing here is secret: keys are derived again
 * from the wallet on every start. The record is the host's own and is not
 * synced anywhere, so another device of the same identity does not see it.
 */

export type MessageKind = "text" | "request" | "accepted" | "unsupported";

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
}

const empty = (): ChatRecord => ({ contacts: [], requests: [], declined: [] });

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
    };
  } catch {
    storage.setItem(`${key}.unreadable`, raw);
    return empty();
  }
}

export function saveChatRecord(
  storage: Storage,
  key: string,
  record: ChatRecord,
): void {
  storage.setItem(key, JSON.stringify(record));
}
