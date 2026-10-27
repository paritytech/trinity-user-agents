import { bytesToHex, type HexString } from "@parity/truapi/scale";
import type { ChatEngine } from "../chat/engine.js";
import {
  abbreviateAccount,
  type Contact,
  type IncomingRequest,
  type StoredMessage,
} from "../chat/store.js";
import type {
  Conversation,
  ConversationEngine,
  ConversationMessage,
  MessageState,
} from "./conversations.js";

/**
 * The Nova chat engine (`../chat/engine.ts`) as a {@link ConversationEngine}:
 * one conversation per peer account, text in both directions.
 *
 * A chat request someone sent shows as that person's conversation with
 * their welcome text; the user's first reply accepts it. Declining, blocking,
 * reactions, files and calls are not offered here yet: the engine keeps what
 * it receives of them, and the host's own chat (the default mode) still has
 * them. A blocked contact does not show, and its session is not followed.
 *
 * Each conversation names its person by account only, as the room's contact:
 * the host names and draws them. The usernames the engine reads on chain to
 * find a peer's chat key stay in the engine.
 *
 * Starting a chat takes the host's reference to the person the user chose,
 * which only {@link NovaConversationOptions.resolveContact}, the host's
 * identity bridge, turns into the account the Nova wire needs.
 */

export interface NovaConversationOptions {
  /** The account behind a start reference the host made. */
  resolveContact?(reference: string): Promise<Uint8Array>;
}

/** The engine calls this adapter uses. */
export type NovaEngine = Pick<
  ChatEngine,
  "snapshot" | "onChange" | "send" | "accept" | "retry" | "startChat"
>;

const OUT_STATE: Record<StoredMessage["status"], MessageState> = {
  pending: "sending",
  sent: "sent",
  acknowledged: "delivered",
  failed: "failed",
  received: "received",
};

function describe(message: StoredMessage, peer: string): ConversationMessage {
  const author =
    message.direction === "out"
      ? ({ kind: "self" } as const)
      : ({ kind: "participant", id: peer } as const);
  const base = {
    id: message.id,
    at: message.at,
    state: OUT_STATE[message.status],
  };
  const system = (text: string): ConversationMessage => ({
    ...base,
    author: { kind: "system" },
    text,
  });
  switch (message.kind) {
    case "text": {
      const files = message.attachments?.length ?? 0;
      const note =
        files === 0
          ? ""
          : ` [${files === 1 ? "a file" : `${files} files`}, not shown here yet]`;
      return { ...base, author, text: `${message.text}${note}`.trim() };
    }
    case "request":
      return {
        ...base,
        author,
        text: message.text === "" ? "Sent a chat request" : message.text,
      };
    case "accepted":
      return system(
        message.direction === "out"
          ? "You accepted the chat request"
          : "Accepted your chat request",
      );
    case "left":
      return system(
        message.direction === "out" ? "You left the chat" : "Left the chat",
      );
    case "call":
      return system(
        `${message.direction === "out" ? "You called" : "Called you"} (calls are not in this chat yet)`,
      );
    case "unsupported":
      return system(message.text);
  }
}

export function novaConversations(
  engine: NovaEngine,
  options: NovaConversationOptions = {},
): ConversationEngine {
  const contactOf = (account: string): Contact | undefined =>
    engine
      .snapshot()
      .contacts.find(
        (contact) => contact.account === account && contact.blocked !== true,
      );
  const requestOf = (account: string): IncomingRequest | undefined =>
    engine.snapshot().requests.find((request) => request.peer === account);

  /** Whether a reply here accepts `request` first. */
  const accepts = (contact: Contact | undefined) =>
    contact === undefined || contact.state !== "active" || contact.peerLeft;

  const conversation = (
    account: string,
    contact: Contact | undefined,
    request: IncomingRequest | undefined,
  ): Conversation => {
    const pendingRequest = request !== undefined && accepts(contact);
    return {
      id: account,
      title: abbreviateAccount(account),
      subtitle: pendingRequest
        ? "Chat request"
        : contact?.state === "requested"
          ? "Request sent"
          : abbreviateAccount(account),
      participants: [
        { id: account, name: abbreviateAccount(account), account },
      ],
      contact: {
        account,
        state: pendingRequest
          ? "Incoming"
          : contact?.state === "requested"
            ? "Outgoing"
            : contact?.peerLeft
              ? "PeerLeft"
              : "Established",
      },
      composer:
        pendingRequest && request.problem !== null
          ? { allowed: false, reason: request.problem }
          : pendingRequest
            ? { allowed: true }
            : contact?.state === "requested"
              ? { allowed: false, reason: "Waiting for them to accept" }
              : contact?.peerLeft
                ? { allowed: false, reason: "They left this chat" }
                : { allowed: true },
    };
  };

  /** Starts in flight, by account, so two starts for one person send one request. */
  const starting = new Map<string, Promise<string>>();
  const starter =
    (resolve: (reference: string) => Promise<Uint8Array>) =>
    async (reference: string, welcome: string): Promise<string> => {
      const account = bytesToHex(await resolve(reference));
      const pending = starting.get(account);
      if (pending !== undefined) return pending;
      const run = (async () => {
        const known = engine
          .snapshot()
          .contacts.find((contact) => contact.account === account);
        if (known?.blocked) throw new Error("You blocked this person.");
        // Someone already in a conversation, or asking for one, is opened, not asked again.
        if (known !== undefined || requestOf(account) !== undefined)
          return account;
        await engine.startChat(account, welcome);
        return account;
      })();
      starting.set(account, run);
      try {
        return await run;
      } finally {
        starting.delete(account);
      }
    };

  return {
    conversations() {
      const snapshot = engine.snapshot();
      const accounts = new Map<string, Conversation>();
      for (const contact of snapshot.contacts)
        if (contact.blocked !== true)
          accounts.set(
            contact.account,
            conversation(
              contact.account,
              contact,
              snapshot.requests.find((r) => r.peer === contact.account),
            ),
          );
      for (const request of snapshot.requests)
        if (!accounts.has(request.peer))
          accounts.set(
            request.peer,
            conversation(request.peer, undefined, request),
          );
      return [...accounts.values()];
    },

    messages(account) {
      const contact = contactOf(account);
      const request = requestOf(account);
      const messages = (contact?.messages ?? []).map((message) =>
        describe(message, account),
      );
      if (request !== undefined && accepts(contact)) {
        messages.push({
          id: request.id,
          author: { kind: "participant", id: account },
          text: request.welcome ?? "Sent a chat request",
          at: request.at,
          state: "received",
        });
        messages.push({
          id: `request-note:${request.id}`,
          author: { kind: "system" },
          text: request.problem ?? "Wants to chat with you. Reply to accept.",
          at: request.at,
          state: "received",
        });
      }
      return messages;
    },

    async send(account, text) {
      const request = requestOf(account);
      if (request !== undefined && accepts(contactOf(account)))
        await engine.accept(request.id);
      const known = new Set(
        contactOf(account)?.messages.map((message) => message.id) ?? [],
      );
      const created = () =>
        contactOf(account)?.messages.find(
          (message) =>
            message.direction === "out" &&
            message.kind === "text" &&
            !known.has(message.id),
        )?.id;
      return new Promise<string>((resolve, reject) => {
        let done = false;
        const settle = (id: string | undefined, error?: unknown) => {
          if (done) return;
          if (id === undefined && error === undefined) return;
          done = true;
          stop();
          if (id !== undefined) resolve(id);
          else reject(error);
        };
        const stop = engine.onChange(() => settle(created()));
        engine.send(account as HexString, text).then(
          () =>
            settle(
              created(),
              new Error("The engine did not keep the message."),
            ),
          (error: unknown) => settle(created(), error),
        );
      });
    },

    retry: (account) => engine.retry(account as HexString),

    ...(options.resolveContact === undefined
      ? {}
      : { start: starter(options.resolveContact) }),

    onChange: (listener) => engine.onChange(listener),
  };
}
