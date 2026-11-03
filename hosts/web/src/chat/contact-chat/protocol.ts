import type { RandomBytes } from "../../invitations/sealing.js";

/**
 * PROTOTYPE. The seam between Contact Chat's engine and the chat protocol it
 * runs. The engine owns the conversation's life: the host request that sets
 * it up, its conversation account, its history and its storage. The
 * protocol owns everything that travels: the setup payloads the hosts carry
 * unread, which topics a side reads and posts on, and the bytes and channel
 * of a post. Neither the engine nor either host reads a payload, so a
 * protocol with other setup material, more or fewer topics, or encryption
 * replaces this one without a change outside it.
 *
 * Both sides post as their own conversation account: the product's own
 * account at a random raw derivation index, made for this conversation only.
 * The core holds its key and signs for the product; the protocol may put its
 * public key in its payloads so each side can tell the other's statements
 * apart. It is not the person's identity and no host names anyone by it.
 *
 * `Pending` and `Session` are what the protocol keeps between calls. The
 * engine stores them as JSON in the product's storage and never reads them.
 */

/** A text message as the engine keeps it. */
export interface ChatText {
  /** 16 random bytes, `0x` hex: the same on both sides, so a copy is recognised. */
  id: string;
  text: string;
  /** Milliseconds since the Unix epoch, as the author's clock said. */
  at: number;
}

/** What a side posts: one statement, which replaces its last one on the same channel. */
export interface Post {
  /** Up to four 32-byte topics, `0x` hex. */
  topics: string[];
  /** 32 bytes, `0x` hex. */
  channel: string;
  /** `0x` hex. */
  data: string;
}

/** A statement the engine heard on a topic it reads. */
export interface Heard {
  /** The account that signed it, `0x` hex. */
  signer: string;
  topics: string[];
  /** `0x` hex; empty when the statement carries no data. */
  data: string;
}

export type Checked<T> = { ok: true; value: T } | { ok: false; reason: string };

/** What a protocol tells the developer tools about a session: its public routing, nothing secret. */
export interface SessionRoutes {
  /** The accounts whose statements this side reads, `0x` hex. */
  peers: string[];
  /** Where this side posts. */
  post: { topics: string[]; channel: string };
}

export interface ChatProtocol<Pending = unknown, Session = unknown> {
  /** The protocol's name and version, as the developer tools show it. */
  readonly name: string;
  /**
   * The most a post carries, when each post publishes a batch of recent
   * messages. The developer tools show it, with the last post's batch, as a
   * protocol detail.
   */
  readonly window?: { messages: number; bytes: number };
  /** The sender's invitation payload, `0x` hex, and what it keeps until the response. */
  invite(
    account: string,
    random: RandomBytes,
  ): {
    payload: string;
    pending: Pending;
  };
  /**
   * The recipient's response to an invitation, and its session, which reads
   * from the moment it exists. Null when this protocol cannot use the
   * invitation.
   */
  respond(
    invitation: string,
    account: string,
    random: RandomBytes,
  ): { response: string; session: Session } | null;
  /** The sender's session from a response its host authenticated. */
  open(pending: Pending, response: string): Checked<Session>;
  /** The topics whose statements this side reads. Any statement on any of them is offered to {@link read}. */
  listen(session: Session): string[];
  /**
   * The post that carries this side's messages, oldest first, and the ids of
   * those it carries: the protocol chooses how many fit.
   */
  post(session: Session, own: ChatText[]): { post: Post; carried: string[] };
  /** The other side's messages in `heard`; none when it is not theirs. */
  read(session: Session, heard: Heard): ChatText[];
  /** The session's public routing, for the developer tools. */
  routes(session: Session): SessionRoutes;
}
