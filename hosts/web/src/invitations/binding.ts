import * as sr25519 from "@scure/sr25519";
import {
  CHALLENGE_BYTES,
  hexBytes,
  isHexOf,
  readPayload,
  readPurpose,
  REQUEST_ID_BYTES,
  type Invitation,
} from "./contract.js";
import {
  digest,
  fromHex,
  fromUtf8,
  openSealed,
  sealTo,
  toHex,
  utf8,
  type RandomBytes,
  type X25519Pair,
} from "./sealing.js";

/**
 * PROTOTYPE, CLASSICAL. What one host signs, seals and checks for an
 * invitation, with real sr25519 signatures and X25519 sealing.
 *
 * - The sender's host signs the envelope for one recipient and seals it to
 *   that recipient's inbox key. The recipient's host opens it and checks the
 *   signature against the contact it knows by that signing key.
 * - The recipient's host signs a proof over both people's keys, the whole
 *   envelope, the challenge, the product, the request id and the exact
 *   response, and seals it to the sender's inbox key. Neither product can
 *   read it.
 * - The sender's host checks that proof against the contact the user chose
 *   when sending, its own stored envelope and the response it is given.
 *
 * Where the keys come from, and that a signing key is a person, is outside
 * this module. In this repository a fixture says so (`src/dev-tools/`); no
 * real identity credential, key directory or PQ suite is involved.
 */

const ENVELOPE = "truapi-web-host/invitation-envelope/0";
const ENVELOPE_SIGNATURE = "truapi-web-host/invitation-signature/0";
const ENVELOPE_SEAL = "truapi-web-host/invitation-seal/0";
const PROOF_SIGNATURE = "truapi-web-host/response-proof/0";
const PROOF_SEAL = "truapi-web-host/response-proof-seal/0";
const RESPONSE = "truapi-web-host/response/0";

/** A host's own keys. Never leave the host. */
export interface HostKeys {
  /** sr25519 secret, 64 bytes. */
  signingSecret: Uint8Array;
  signingPublic: Uint8Array;
  inbox: X25519Pair;
}

/** Another person's public keys, as the host knows them, in `0x` hex. */
export interface PeerKeys {
  signingPublic: string;
  inboxPublic: string;
}

export function hostKeys(signingSeed: Uint8Array, inbox: X25519Pair): HostKeys {
  const signingSecret = sr25519.secretFromSeed(signingSeed);
  return {
    signingSecret,
    signingPublic: sr25519.getPublicKey(signingSecret),
    inbox,
  };
}

export const peerKeysOf = (keys: HostKeys): PeerKeys => ({
  signingPublic: toHex(keys.signingPublic),
  inboxPublic: toHex(keys.inbox.public),
});

/** What the sender's host sends: its product's invitation, and what the recipient's host shows. */
export interface Envelope {
  product: { id: string; name: string };
  purpose: string;
  createdAt: number;
  invitation: Invitation;
}

export function envelopeDigest(envelope: Envelope): string {
  const { product, purpose, createdAt, invitation } = envelope;
  return toHex(
    digest(
      ENVELOPE,
      product.id,
      product.name,
      purpose,
      String(createdAt),
      invitation.requestId,
      invitation.challenge,
      String(invitation.expiresAt),
      invitation.setup,
    ),
  );
}

export const responseDigest = (response: string): string =>
  toHex(digest(RESPONSE, response));

function verify(message: Uint8Array, signature: string, key: string): boolean {
  try {
    return sr25519.verify(message, fromHex(signature), fromHex(key));
  } catch {
    return false;
  }
}

const envelopeMessage = (from: string, to: string, envelope: Envelope) =>
  digest(ENVELOPE_SIGNATURE, from, to, envelopeDigest(envelope));

/** Sign `envelope` for `to` and seal it to `to`'s inbox. */
export function sealEnvelope(
  self: HostKeys,
  to: PeerKeys,
  envelope: Envelope,
  random: RandomBytes,
): string {
  const from = toHex(self.signingPublic);
  const signature = toHex(
    sr25519.sign(
      self.signingSecret,
      envelopeMessage(from, to.signingPublic, envelope),
      random(32),
    ),
  );
  return toHex(
    sealTo(
      fromHex(to.inboxPublic),
      utf8(JSON.stringify({ from, envelope, signature })),
      ENVELOPE_SEAL,
      random,
    ),
  );
}

function readEnvelope(value: unknown): Envelope | null {
  if (typeof value !== "object" || value === null) return null;
  const item = value as Partial<Envelope>;
  const invitation = item.invitation as Partial<Invitation> | undefined;
  if (
    typeof item.product?.id !== "string" ||
    typeof item.product.name !== "string" ||
    !readPurpose(item.purpose).ok ||
    typeof item.createdAt !== "number" ||
    !Number.isFinite(item.createdAt) ||
    !isHexOf(invitation?.requestId, REQUEST_ID_BYTES) ||
    !isHexOf(invitation.challenge, CHALLENGE_BYTES) ||
    typeof invitation.expiresAt !== "number" ||
    !Number.isFinite(invitation.expiresAt) ||
    !readPayload(invitation.setup, "setup data").ok
  )
    return null;
  return {
    product: { id: item.product.id, name: item.product.name },
    purpose: item.purpose as string,
    createdAt: item.createdAt,
    invitation: {
      requestId: invitation.requestId,
      challenge: invitation.challenge,
      expiresAt: invitation.expiresAt,
      setup: invitation.setup as string,
    },
  };
}

export type Opened =
  | { ok: true; from: string; envelope: Envelope }
  | { ok: false; reason: string };

/**
 * Open what arrived for this host and check who signed it. `known` gives the
 * keys of a contact by signing key; anyone else is refused.
 */
export function openEnvelope(
  self: HostKeys,
  sealed: string,
  known: (signingPublic: string) => PeerKeys | undefined,
): Opened {
  const refuse = (reason: string): Opened => ({ ok: false, reason });
  if (hexBytes(sealed) === null) return refuse("not hex");
  const plain = openSealed(self.inbox, fromHex(sealed), ENVELOPE_SEAL);
  if (plain === null) return refuse("not sealed to this host");
  let parsed: { from?: unknown; envelope?: unknown; signature?: unknown };
  try {
    parsed = JSON.parse(fromUtf8(plain)) as typeof parsed;
  } catch {
    return refuse("not an envelope");
  }
  const envelope = readEnvelope(parsed.envelope);
  if (envelope === null) return refuse("not an envelope");
  if (typeof parsed.from !== "string" || typeof parsed.signature !== "string")
    return refuse("unsigned");
  const sender = known(parsed.from.toLowerCase());
  if (sender === undefined) return refuse("the sender is not a contact");
  const message = envelopeMessage(
    sender.signingPublic,
    toHex(self.signingPublic),
    envelope,
  );
  if (!verify(message, parsed.signature, sender.signingPublic))
    return refuse("the signature does not check");
  return { ok: true, from: sender.signingPublic, envelope };
}

const proofMessage = (
  inviter: string,
  responder: string,
  envelope: Envelope,
  response: string,
) =>
  digest(
    PROOF_SIGNATURE,
    inviter,
    responder,
    envelopeDigest(envelope),
    envelope.invitation.challenge,
    envelope.product.id,
    envelope.invitation.requestId,
    responseDigest(response),
  );

/** The proof that this host's user answered `envelope` from `inviter` with `response`, sealed to `inviter`. */
export function issueProof(
  self: HostKeys,
  inviter: PeerKeys,
  envelope: Envelope,
  response: string,
  random: RandomBytes,
): string {
  const responder = toHex(self.signingPublic);
  const signature = toHex(
    sr25519.sign(
      self.signingSecret,
      proofMessage(inviter.signingPublic, responder, envelope, response),
      random(32),
    ),
  );
  return toHex(
    sealTo(
      fromHex(inviter.inboxPublic),
      utf8(
        JSON.stringify({
          responder,
          requestId: envelope.invitation.requestId,
          envelope: envelopeDigest(envelope),
          response: responseDigest(response),
          signature,
        }),
      ),
      PROOF_SEAL,
      random,
    ),
  );
}

/**
 * Null when `proof` shows that `expected` answered this host's `envelope`
 * with exactly `response`; else why not.
 */
export function checkProof(
  self: HostKeys,
  expected: PeerKeys,
  envelope: Envelope,
  response: string,
  proof: string,
): string | null {
  if (hexBytes(proof) === null) return "the proof is not hex";
  const plain = openSealed(self.inbox, fromHex(proof), PROOF_SEAL);
  if (plain === null) return "the proof is not sealed to this host";
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(fromUtf8(plain)) as Record<string, unknown>;
  } catch {
    return "the proof is malformed";
  }
  if (typeof parsed.signature !== "string") return "the proof is malformed";
  if (parsed.responder !== expected.signingPublic)
    return "the proof is from someone other than the contact invited";
  if (
    parsed.requestId !== envelope.invitation.requestId ||
    parsed.envelope !== envelopeDigest(envelope)
  )
    return "the proof is for another invitation";
  if (parsed.response !== responseDigest(response))
    return "the proof is for another response";
  const message = proofMessage(
    toHex(self.signingPublic),
    expected.signingPublic,
    envelope,
    response,
  );
  return verify(message, parsed.signature, expected.signingPublic)
    ? null
    : "the proof's signature does not check";
}
