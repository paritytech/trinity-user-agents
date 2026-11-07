import { bytes, concat, text, u64, utf8 } from "../../platform/codec.js";
import { blake2b256, keyedBlake2b256 } from "../../platform/crypto.js";
import { PROTOCOL_EPOCH_SECONDS } from "../../platform/statement.js";

/**
 * Topics and channels (Android `CommunicationTopicDerivation.kt`,
 * `SessionIdParams.kt`, `StatementChannels.kt`, `ChatRequestTopicDerivation.kt`,
 * `StatementTimestamp.kt`; iOS `PeerSessionIdFactory.swift`). Contact pins are
 * null for chat contacts (Android `Contact.kt` default), so both pin slots are empty.
 */

const SEPARATOR = utf8("/");

/** `blake2b256(key = shared, "session" ‖ first ‖ second ‖ "/" ‖ pinA ‖ "/" ‖ pinB)`, the writer's topic. */
export const sessionTopic = (
  shared: Uint8Array,
  sender: Uint8Array,
  receiver: Uint8Array,
): Uint8Array =>
  keyedBlake2b256(
    shared,
    concat(utf8("session"), sender, receiver, SEPARATOR, SEPARATOR),
  );

export const requestChannel = (topic: Uint8Array): Uint8Array =>
  keyedBlake2b256(topic, utf8("request"));

export const responseChannel = (topic: Uint8Array): Uint8Array =>
  keyedBlake2b256(topic, utf8("response"));

/** Topic 1 of a chat request: SCALE `("chat-request", acceptor: Vec<u8>, day: u64)`, hashed. */
export const requestDayTopic = (
  acceptor: Uint8Array,
  day: bigint,
): Uint8Array =>
  blake2b256(concat(text("chat-request"), bytes(acceptor), u64(day)));

/** Topic 2 of a chat request: SCALE `("chat-request", acceptor: Vec<u8>)`, hashed. */
export const requestFullTopic = (acceptor: Uint8Array): Uint8Array =>
  blake2b256(concat(text("chat-request"), bytes(acceptor)));

/** Topic 3 of a chat request: keyed with the identity chat secret, requester first. */
export const requestSessionTopic = (
  shared: Uint8Array,
  requester: Uint8Array,
  acceptor: Uint8Array,
): Uint8Array =>
  keyedBlake2b256(
    shared,
    concat(utf8("chat-request"), requester, acceptor, SEPARATOR, SEPARATOR),
  );

const SECONDS_PER_DAY = 86_400;

/** The request day: whole days since the protocol epoch (2025-11-15 00:00 UTC). Used only in request topics. */
export const protocolDay = (nowMs: number): bigint =>
  BigInt(
    Math.floor(
      Math.max(0, Math.floor(nowMs / 1000) - PROTOCOL_EPOCH_SECONDS) /
        SECONDS_PER_DAY,
    ),
  );

/** The allowance period of a notification claim: whole UTC days since the Unix epoch (`statement_allowance/slot.rs` `current_period`). */
export const claimPeriod = (nowMs: number): number =>
  Math.floor(Math.floor(nowMs / 1000) / SECONDS_PER_DAY);
