import { cryptoRandom, type RandomBytes } from "./crypto.js";
import type { ChatKeys } from "./keys.js";
import {
  decodeSessionData,
  identityProofMatches,
  requestChannel,
  requestDayTopic,
  requestPlaintext,
  requestSessionTopic,
  requestTopic,
  responseChannel,
  sealRequestPlaintext,
  sealSessionPlaintext,
  sessionTopic,
  unsealRequest,
  unsealSession,
} from "./protocol.js";
import { encodeSigned } from "./statement.js";

/**
 * What the Nova chat engine does with the wallet's chat keys, as operations
 * that never hand out a key: the identity account's secret, the X25519 chat
 * secret, and every secret agreed with a peer stay with whoever implements
 * this. The engine gets public keys, public routing topics, plaintext it
 * must read, and statements signed for the chat's own topics and channels.
 *
 * Signing is not general. The implementation builds each statement itself
 * from the peer's keys: a session statement goes on the pair's outgoing
 * topic and request or response channel, with data sealed here from
 * session data that decodes; a chat request is built here from its fields.
 * No operation signs bytes the caller chose.
 *
 * This is the Nova protocol's identity, not a general chat interface: another
 * engine (libsignal, OpenMLS) keeps its own keys and state.
 */
export interface NovaIdentity {
  /** The identity account: requests are addressed to it and it signs every statement. */
  readonly identity: Uint8Array;
  /** The X25519 chat key peers find on chain. */
  readonly chatPublic: Uint8Array;
  /** Decrypt a chat request's data, sealed to this chat key. */
  openRequest(data: Uint8Array): Promise<Uint8Array>;
  /** Whether a version 2 request's identity proof matches the sender's chat key. */
  identityProofMatches(
    peerChatKey: Uint8Array,
    proof: IdentityProof,
  ): Promise<boolean>;
  /** The pair's session topics: the peer writes on `incoming`, this side on `outgoing`. */
  sessionTopics(peer: Uint8Array, peerChatKey: Uint8Array): Promise<PairTopics>;
  /** Decrypt a session statement's data from the peer with this chat key. */
  openSession(peerChatKey: Uint8Array, sealed: Uint8Array): Promise<Uint8Array>;
  /** Seal session data for the peer and sign it as a statement on this side's topic. */
  sessionStatement(request: SessionStatementRequest): Promise<Uint8Array>;
  /** Build, sign and seal a version 1 chat request to the peer, as a statement. */
  requestStatement(request: ChatRequestStatement): Promise<SignedRequest>;
}

export interface IdentityProof {
  identity: Uint8Array;
  signer: Uint8Array;
  proof: Uint8Array;
}

export interface PairTopics {
  incoming: Uint8Array;
  outgoing: Uint8Array;
}

export interface SessionStatementRequest {
  peer: Uint8Array;
  peerChatKey: Uint8Array;
  /** Encoded session data: a request goes on the request channel, a response on the response channel. */
  plaintext: Uint8Array;
  expiry: bigint;
}

export interface ChatRequestStatement {
  peer: Uint8Array;
  peerChatKey: Uint8Array;
  id: string;
  timestampMs: bigint;
  welcome: string | null;
  expiry: bigint;
  /** The request day the statement is filed under. */
  day: bigint;
}

export interface SignedRequest {
  statement: Uint8Array;
  /** The request's signed plaintext, before it was sealed. */
  plaintext: Uint8Array;
}

/** A {@link NovaIdentity} over keys held in this realm, as the host's own chat uses it. */
export function localNovaIdentity(
  keys: ChatKeys,
  random: RandomBytes = cryptoRandom,
): NovaIdentity {
  const me = keys.identity;
  const sign = (payload: Uint8Array) => keys.sign(payload);
  return {
    identity: me,
    chatPublic: keys.chatPublic,
    async openRequest(data) {
      return unsealRequest(data, (ephemeral) => keys.agree(ephemeral));
    },
    async identityProofMatches(peerChatKey, { identity, signer, proof }) {
      return identityProofMatches(
        keys.agree(peerChatKey),
        identity,
        signer,
        proof,
      );
    },
    async sessionTopics(peer, peerChatKey) {
      const secret = keys.agree(peerChatKey);
      return {
        incoming: sessionTopic(secret, peer, me),
        outgoing: sessionTopic(secret, me, peer),
      };
    },
    async openSession(peerChatKey, sealed) {
      return unsealSession(keys.agree(peerChatKey), sealed);
    },
    async sessionStatement({ peer, peerChatKey, plaintext, expiry }) {
      const data = decodeSessionData(plaintext);
      if (data.kind === "multi-device")
        throw new Error("This identity does not send multi-device envelopes.");
      const secret = keys.agree(peerChatKey);
      const topic = sessionTopic(secret, me, peer);
      return encodeSigned(
        {
          expiry,
          channel:
            data.kind === "request"
              ? requestChannel(topic)
              : responseChannel(topic),
          topics: [topic],
          data: sealSessionPlaintext(secret, plaintext, random),
        },
        me,
        sign,
      );
    },
    async requestStatement(request) {
      const { peer, peerChatKey } = request;
      const plaintext = requestPlaintext(
        {
          id: request.id,
          timestampMs: request.timestampMs,
          content: { version: 1, welcome: request.welcome },
        },
        peer,
        me,
        sign,
      );
      const statement = encodeSigned(
        {
          expiry: request.expiry,
          channel: null,
          topics: [
            requestDayTopic(peer, request.day),
            requestTopic(peer),
            requestSessionTopic(keys.agree(peerChatKey), me, peer),
          ],
          data: sealRequestPlaintext(plaintext, peerChatKey, random),
        },
        me,
        sign,
      );
      return { statement, plaintext };
    },
  };
}
