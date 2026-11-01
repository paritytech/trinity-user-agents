import {
  fromHex,
  fromUtf8,
  toHex,
  utf8,
} from "../../../src/invitations/sealing.js";
import type { ChatProtocol, ChatText, Checked, Heard } from "./protocol.js";

/**
 * DEV ONLY. `simple-text/1`: plain text between two conversation accounts,
 * with no encryption of its own. It is the first protocol behind
 * `./protocol.ts`, to have real messages before a chosen library (MLS,
 * libsignal or another) replaces it.
 *
 * - The invitation names the sender's conversation account and a random
 *   challenge.
 * - The response repeats the challenge, so it answers that one invitation,
 *   and names the recipient's conversation account, two random topics and
 *   two random channels the recipient made: `tAB`/`cAB` for the sender's
 *   posts and `tBA`/`cBA` for its own.
 * - Each side keeps one statement up: on its topic and channel, signed by its
 *   conversation account, holding its last {@link WINDOW_MESSAGES} messages
 *   as UTF-8 JSON, fewer when they would pass {@link WINDOW_MAX_BYTES}. Each
 *   post replaces the last one, so the network keeps a short recent window,
 *   not a history: an account's general allowance keeps two statements.
 *   Each side keeps its own history in its product's storage.
 * - A side reads only statements its peer's account signed on its own topic,
 *   and keeps each message once by its id.
 *
 * Anyone who reads the Statement Store reads these messages, their topics,
 * channels, accounts, sizes and times. The hosts authenticate and seal the
 * setup; the text after it is public.
 */

const VERSION = "contact-chat/simple-text/1";

/** The longest message, in UTF-16 code units. */
export const TEXT_MAX_CHARS = 2_000;
/** The most messages one post carries. */
export const WINDOW_MESSAGES = 20;
/** The most data one post carries, in bytes: well inside one statement. */
export const WINDOW_MAX_BYTES = 16_384;

/** What the sender keeps until the response. */
export interface Pending {
  account: string;
  challenge: string;
}

/** What each side keeps of a connected conversation. */
export interface Session {
  /** The topic this side reads. */
  listen: string;
  /** The topic and channel this side posts on. */
  send: string;
  channel: string;
  /** The other side's conversation account. */
  peer: string;
}

const encode = (value: unknown): string => toHex(utf8(JSON.stringify(value)));

function decode(hex: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(fromUtf8(fromHex(hex)));
    return typeof value === "object" && value !== null
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

const isBytes = (value: unknown, length: number): value is string =>
  typeof value === "string" &&
  value.length === 2 + 2 * length &&
  /^0x[0-9a-f]*$/.test(value);

function readText(value: unknown): ChatText | null {
  const entry = value as Record<string, unknown> | null;
  return typeof entry === "object" &&
    entry !== null &&
    isBytes(entry.id, 16) &&
    typeof entry.text === "string" &&
    entry.text !== "" &&
    entry.text.length <= TEXT_MAX_CHARS &&
    typeof entry.at === "number" &&
    Number.isFinite(entry.at)
    ? { id: entry.id, text: entry.text, at: entry.at }
    : null;
}

/** The invitation's sender account and challenge, or null when this protocol cannot use it. */
function readInvitation(payload: string): Pending | null {
  const value = decode(payload);
  return value?.version === VERSION &&
    value.kind === "invitation" &&
    isBytes(value.account, 32) &&
    isBytes(value.challenge, 32)
    ? { account: value.account, challenge: value.challenge }
    : null;
}

const encodeWindow = (messages: ChatText[]) =>
  encode({
    version: VERSION,
    kind: "messages",
    messages: messages.map(({ id, text, at }) => ({ id, text, at })),
  });

export const simpleText: ChatProtocol<Pending, Session> = {
  name: VERSION,
  window: { messages: WINDOW_MESSAGES, bytes: WINDOW_MAX_BYTES },
  notice: `Messages here are not encrypted: anyone who reads the Statement Store can read them. The network keeps only each side's last ${WINDOW_MESSAGES} messages, for a short time; this app keeps your history.`,

  invite(account, random) {
    const challenge = toHex(random(32));
    return {
      payload: encode({
        version: VERSION,
        kind: "invitation",
        account,
        challenge,
      }),
      pending: { account, challenge },
    };
  },

  respond(invitation, account, random) {
    const read = readInvitation(invitation);
    if (read === null || read.account === account) return null;
    const [tAB, tBA, cAB, cBA] = [0, 1, 2, 3].map(() => toHex(random(32)));
    return {
      response: encode({
        version: VERSION,
        kind: "response",
        challenge: read.challenge,
        account,
        tAB,
        tBA,
        cAB,
        cBA,
      }),
      session: { listen: tAB!, send: tBA!, channel: cBA!, peer: read.account },
    };
  },

  open(pending, response): Checked<Session> {
    const value = decode(response);
    if (
      value?.version !== VERSION ||
      value.kind !== "response" ||
      !isBytes(value.account, 32) ||
      !isBytes(value.tAB, 32) ||
      !isBytes(value.tBA, 32) ||
      !isBytes(value.cAB, 32) ||
      !isBytes(value.cBA, 32)
    )
      return { ok: false, reason: "not a simple-text response" };
    if (value.challenge !== pending.challenge)
      return { ok: false, reason: "the response answers another invitation" };
    if (value.tAB === value.tBA)
      return {
        ok: false,
        reason: "the response gave one topic for both sides",
      };
    if (value.account === pending.account)
      return {
        ok: false,
        reason: "the response names this side's own account",
      };
    return {
      ok: true,
      value: {
        listen: value.tBA,
        send: value.tAB,
        channel: value.cAB,
        peer: value.account,
      },
    };
  },

  listen: (session) => [session.listen],

  routes: (session) => ({
    peers: [session.peer],
    post: { topics: [session.send], channel: session.channel },
  }),

  post(session, own) {
    const tooLong = own.find((message) => message.text.length > TEXT_MAX_CHARS);
    if (tooLong !== undefined)
      throw new Error(
        `A message has at most ${TEXT_MAX_CHARS} characters in this chat.`,
      );
    let carried = own.slice(-WINDOW_MESSAGES);
    while (
      carried.length > 1 &&
      (encodeWindow(carried).length - 2) / 2 > WINDOW_MAX_BYTES
    )
      carried = carried.slice(1);
    return {
      post: {
        topics: [session.send],
        channel: session.channel,
        data: encodeWindow(carried),
      },
      carried: carried.map((message) => message.id),
    };
  },

  read(session, heard: Heard): ChatText[] {
    if (heard.signer !== session.peer || !heard.topics.includes(session.listen))
      return [];
    const value = decode(heard.data);
    if (
      value?.version !== VERSION ||
      value.kind !== "messages" ||
      !Array.isArray(value.messages)
    )
      return [];
    return value.messages
      .slice(-WINDOW_MESSAGES)
      .map(readText)
      .filter((message): message is ChatText => message !== null);
  },
};
