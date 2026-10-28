import {
  digest,
  fromHex,
  fromUtf8,
  toHex,
  utf8,
  x25519Agree,
  x25519Pair,
  type RandomBytes,
} from "../../../src/invitations/sealing.js";

/**
 * DEV ONLY. Example Chat's own setup protocol: a toy that shows where a
 * product's check sits beside its host's. It is not libsignal, MLS or
 * SimpleX, and no chat library or suite is chosen by it. Everything here is
 * product payload; the hosts carry it unchanged.
 *
 * - The sender's product makes a protocol key and a challenge, and keeps the
 *   secret. Its invitation payload holds the public key and the challenge.
 * - The recipient's product makes its own protocol key and both conversation
 *   topics, `T_AB` (sender to recipient) and `T_BA` (recipient to sender),
 *   and a confirmation keyed with the two protocol keys' X25519 secret over
 *   the whole invitation payload and everything it answers with.
 * - The sender's product checks the confirmation and keeps the topics.
 */

const INVITATION = "example-chat-invitation/1";
const RESPONSE = "example-chat-response/1";

/** What the sender's product keeps of an invitation it made. Never leaves the product. */
export interface SenderSecrets {
  protocolSecret: string;
  /** The invitation payload, as sent. */
  payload: string;
}

/** The conversation topics the recipient's product made. */
export interface Topics {
  /** Sender to recipient. */
  tAB: string;
  /** Recipient to sender. */
  tBA: string;
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

const isKey = (value: unknown): value is string =>
  typeof value === "string" && /^0x[0-9a-f]{64}$/.test(value);

/** A new invitation payload for the host to send, and the secrets to keep. */
export function createInvitation(random: RandomBytes): {
  payload: string;
  secrets: SenderSecrets;
} {
  const protocol = x25519Pair(random(32));
  const payload = encode({
    version: INVITATION,
    protocolKey: toHex(protocol.public),
    challenge: toHex(random(32)),
  });
  return {
    payload,
    secrets: { protocolSecret: toHex(protocol.secret), payload },
  };
}

/** The invitation's protocol key, or null when Example Chat cannot use the payload. */
export function readInvitation(
  payload: string,
): { protocolKey: string } | null {
  const value = decode(payload);
  return value?.version === INVITATION &&
    isKey(value.protocolKey) &&
    isKey(value.challenge)
    ? { protocolKey: value.protocolKey }
    : null;
}

const confirmation = (
  shared: Uint8Array,
  invitation: string,
  protocolKey: string,
  topics: Topics,
): string =>
  toHex(
    digest(
      "example-chat-confirm/1",
      shared,
      invitation,
      protocolKey,
      topics.tAB,
      topics.tBA,
    ),
  );

/** The recipient's response payload, and what it keeps, or null when the invitation is unusable. */
export function respond(
  invitation: string,
  random: RandomBytes,
): { response: string; protocolSecret: string; topics: Topics } | null {
  const read = readInvitation(invitation);
  if (read === null) return null;
  const protocol = x25519Pair(random(32));
  const protocolKey = toHex(protocol.public);
  const topics: Topics = { tAB: toHex(random(32)), tBA: toHex(random(32)) };
  const shared = x25519Agree(protocol.secret, fromHex(read.protocolKey));
  return {
    response: encode({
      version: RESPONSE,
      protocolKey,
      ...topics,
      confirm: confirmation(shared, invitation, protocolKey, topics),
    }),
    protocolSecret: toHex(protocol.secret),
    topics,
  };
}

/** Example Chat's own check of a response its host delivered. */
export function checkResponse(
  secrets: SenderSecrets,
  response: string,
): { ok: true; topics: Topics } | { ok: false; reason: string } {
  const value = decode(response);
  if (
    value?.version !== RESPONSE ||
    !isKey(value.protocolKey) ||
    !isKey(value.tAB) ||
    !isKey(value.tBA) ||
    !isKey(value.confirm)
  )
    return { ok: false, reason: "not an Example Chat response" };
  const topics: Topics = { tAB: value.tAB, tBA: value.tBA };
  const shared = x25519Agree(
    fromHex(secrets.protocolSecret),
    fromHex(value.protocolKey),
  );
  return confirmation(shared, secrets.payload, value.protocolKey, topics) ===
    value.confirm
    ? { ok: true, topics }
    : { ok: false, reason: "the confirmation does not check" };
}
