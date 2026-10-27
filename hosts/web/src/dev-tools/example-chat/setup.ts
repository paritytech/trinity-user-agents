import type { Invitation } from "../../invitations/contract.js";
import {
  digest,
  fromHex,
  fromUtf8,
  openSealed,
  sealTo,
  toHex,
  utf8,
  x25519Agree,
  x25519Pair,
  type RandomBytes,
} from "../../invitations/sealing.js";

/**
 * DEV ONLY. Example Chat's own setup protocol: a toy that shows where a
 * product's check sits beside the host's. It is not libsignal, MLS or
 * SimpleX, and no chat library or suite is chosen by it.
 *
 * - The sender's product makes a request id, a challenge, a reply topic
 *   `T_BA`, a reply key `e_A` and a protocol key, and keeps the secrets.
 * - The recipient's product makes its own protocol key and topic `T_AB`, and
 *   a confirmation keyed with the two protocol keys' X25519 secret over the
 *   whole invitation. Its response holds these, never a person's identity.
 * - The reply, the response with the host's sealed proof, is sealed to `e_A`
 *   and posted on `T_BA`.
 * - The sender's product opens the reply and checks the confirmation.
 */

const SETUP = "example-chat-setup/0";
const RESPONSE = "example-chat-response/0";
const REPLY_SEAL = "example-chat-reply/0";

/** What the sender's product keeps of an invitation it made. Never leaves the product. */
export interface SenderSecrets {
  requestId: string;
  challenge: string;
  replyTopic: string;
  replySecret: string;
  protocolSecret: string;
}

/** What the setup data carries. */
export interface Setup {
  replyTopic: string;
  replyKey: string;
  protocolKey: string;
}

/** What the response carries. */
export interface Response {
  requestId: string;
  protocolKey: string;
  topic: string;
  confirm: string;
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

/** A new invitation for the host to send, and the secrets to keep. */
export function createInvitation(random: RandomBytes): {
  request: Omit<Invitation, "expiresAt">;
  secrets: SenderSecrets;
} {
  const reply = x25519Pair(random(32));
  const protocol = x25519Pair(random(32));
  const secrets: SenderSecrets = {
    requestId: toHex(random(16)),
    challenge: toHex(random(32)),
    replyTopic: toHex(random(32)),
    replySecret: toHex(reply.secret),
    protocolSecret: toHex(protocol.secret),
  };
  const setup: Setup = {
    replyTopic: secrets.replyTopic,
    replyKey: toHex(reply.public),
    protocolKey: toHex(protocol.public),
  };
  return {
    request: {
      requestId: secrets.requestId,
      challenge: secrets.challenge,
      setup: encode({ version: SETUP, ...setup }),
    },
    secrets,
  };
}

/** The setup data, or null when Example Chat cannot use it. */
export function readSetup(hex: string): Setup | null {
  const value = decode(hex);
  if (
    value?.version !== SETUP ||
    !isKey(value.replyTopic) ||
    !isKey(value.replyKey) ||
    !isKey(value.protocolKey)
  )
    return null;
  return {
    replyTopic: value.replyTopic,
    replyKey: value.replyKey,
    protocolKey: value.protocolKey,
  };
}

function readResponse(hex: string): Response | null {
  const value = decode(hex);
  if (
    value?.version !== RESPONSE ||
    typeof value.requestId !== "string" ||
    !isKey(value.protocolKey) ||
    !isKey(value.topic) ||
    !isKey(value.confirm)
  )
    return null;
  return {
    requestId: value.requestId,
    protocolKey: value.protocolKey,
    topic: value.topic,
    confirm: value.confirm,
  };
}

const confirmation = (
  shared: Uint8Array,
  invitation: Invitation,
  topic: string,
  protocolKey: string,
): string =>
  toHex(
    digest(
      "example-chat-confirm/0",
      shared,
      invitation.requestId,
      invitation.challenge,
      String(invitation.expiresAt),
      invitation.setup,
      topic,
      protocolKey,
    ),
  );

/** The recipient's response to `invitation`, and what it keeps, or null when the setup is unusable. */
export function respond(
  invitation: Invitation,
  random: RandomBytes,
): {
  response: string;
  topic: string;
  protocolSecret: string;
  setup: Setup;
} | null {
  const setup = readSetup(invitation.setup);
  if (setup === null) return null;
  const protocol = x25519Pair(random(32));
  const topic = toHex(random(32));
  const protocolKey = toHex(protocol.public);
  const shared = x25519Agree(protocol.secret, fromHex(setup.protocolKey));
  return {
    response: encode({
      version: RESPONSE,
      requestId: invitation.requestId,
      protocolKey,
      topic,
      confirm: confirmation(shared, invitation, topic, protocolKey),
    }),
    topic,
    protocolSecret: toHex(protocol.secret),
    setup,
  };
}

/** The reply posted on `T_BA`: the response and the host's proof, sealed to `e_A`. */
export function sealReply(
  replyKey: string,
  response: string,
  proof: string,
  random: RandomBytes,
): string {
  return toHex(
    sealTo(
      fromHex(replyKey),
      utf8(JSON.stringify({ response, proof })),
      REPLY_SEAL,
      random,
    ),
  );
}

/** Open a reply with the sender's reply secret, or null when it is not one. */
export function openReply(
  secrets: SenderSecrets,
  data: string,
): { response: string; proof: string } | null {
  let sealed: Uint8Array;
  try {
    sealed = fromHex(data);
  } catch {
    return null;
  }
  const plain = openSealed(
    x25519Pair(fromHex(secrets.replySecret)),
    sealed,
    REPLY_SEAL,
  );
  if (plain === null) return null;
  try {
    const value = JSON.parse(fromUtf8(plain)) as Record<string, unknown>;
    return typeof value.response === "string" && typeof value.proof === "string"
      ? { response: value.response, proof: value.proof }
      : null;
  } catch {
    return null;
  }
}

/** Example Chat's own check of a response, after its host verified the proof. */
export function checkResponse(
  secrets: SenderSecrets,
  invitation: Invitation,
  response: string,
):
  | { ok: true; topic: string; peerKey: string }
  | { ok: false; reason: string } {
  const read = readResponse(response);
  if (read === null)
    return { ok: false, reason: "not an Example Chat response" };
  if (
    read.requestId !== secrets.requestId ||
    invitation.requestId !== secrets.requestId ||
    invitation.challenge !== secrets.challenge
  )
    return { ok: false, reason: "for another invitation" };
  const shared = x25519Agree(
    fromHex(secrets.protocolSecret),
    fromHex(read.protocolKey),
  );
  return confirmation(shared, invitation, read.topic, read.protocolKey) ===
    read.confirm
    ? { ok: true, topic: read.topic, peerKey: read.protocolKey }
    : { ok: false, reason: "the confirmation does not check" };
}
