import { blake2b } from "@noble/hashes/blake2.js";
import * as sr25519 from "@scure/sr25519";
import type {
  AuthEvidence,
  EvidenceKind,
  IdentityAuth,
  IdentityKeys,
  Person,
} from "../invitations/ports.js";
import {
  digest,
  fromHex,
  openSealed,
  toHex,
  x25519Agree,
  x25519Pair,
  type RandomBytes,
  type X25519Pair,
} from "../invitations/sealing.js";

/**
 * DEV ONLY. REFERENCE FIXTURE PROFILE of `IdentityAuth`, over made-up keys.
 * It gives v3's `AuthEvidence` its fields with established primitives so the
 * prototype can authenticate for real; it is not a reviewed or agreed
 * profile, and no real user signs with it.
 *
 * - `identityMac`: BLAKE2b-256 keyed with the static X25519 shared secret of
 *   the two identities, over the kind and the transcript. Either side can
 *   make it, so it says only "one of us"; the transcript's identity order and
 *   the signature tell the direction.
 * - `messageSignature`: sr25519, by the identity's signing key, over the kind,
 *   the transcript and the MAC.
 *
 * Open choices this does not settle: which identity key a real host signs
 * with (the native app's wallet/UID account, or another), its encoding, the
 * exact transcript, pseudonymous identities per contact (this profile shows
 * one identity to everyone), and any post-quantum part.
 */

/** A made-up identity's secrets. Never leave the dev tools. */
export interface FixtureCredential {
  /** sr25519 secret key, 64 bytes. */
  signingSecret: Uint8Array;
  agreement: X25519Pair;
}

const KIND_BYTE: Record<EvidenceKind, number> = { Request: 1, Reply: 2 };

export function fixtureCredential(
  signingSeed: Uint8Array,
  agreementSecret: Uint8Array,
): FixtureCredential {
  return {
    signingSecret: sr25519.secretFromSeed(signingSeed),
    agreement: x25519Pair(agreementSecret),
  };
}

export function publicIdentity(credential: FixtureCredential): IdentityKeys {
  return {
    signingPublic: toHex(sr25519.getPublicKey(credential.signingSecret)),
    agreementPublic: toHex(credential.agreement.public),
  };
}

function identityMac(
  credential: FixtureCredential,
  peer: IdentityKeys,
  kind: EvidenceKind,
  transcript: Uint8Array,
): Uint8Array {
  const shared = x25519Agree(
    credential.agreement.secret,
    fromHex(peer.agreementPublic),
  );
  const message = new Uint8Array(1 + transcript.length);
  message[0] = KIND_BYTE[kind];
  message.set(transcript, 1);
  return blake2b(message, { key: shared, dkLen: 32 });
}

const signedMessage = (
  kind: EvidenceKind,
  transcript: Uint8Array,
  mac: Uint8Array,
): Uint8Array =>
  digest("truapi-web-host/dev-fixture/auth-signature/1", kind, transcript, mac);

function equal(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1)
    difference |= left[index]! ^ right[index]!;
  return difference === 0;
}

export class FixtureIdentityAuth implements IdentityAuth {
  private readonly identity: IdentityKeys;

  constructor(
    private readonly credential: FixtureCredential,
    private readonly random: RandomBytes,
  ) {
    this.identity = publicIdentity(credential);
  }

  identityFor(): IdentityKeys {
    return this.identity;
  }

  evidence(
    kind: EvidenceKind,
    transcript: Uint8Array,
    peer: Person,
  ): AuthEvidence {
    const mac = identityMac(this.credential, peer.identity, kind, transcript);
    const signature = sr25519.sign(
      this.credential.signingSecret,
      signedMessage(kind, transcript, mac),
      this.random(32),
    );
    return {
      kind,
      identityMac: toHex(mac),
      messageSignature: toHex(signature),
    };
  }

  check(
    kind: EvidenceKind,
    transcript: Uint8Array,
    evidence: AuthEvidence,
    peer: Person,
  ): boolean {
    if (evidence.kind !== kind) return false;
    try {
      const mac = fromHex(evidence.identityMac);
      return (
        equal(
          mac,
          identityMac(this.credential, peer.identity, kind, transcript),
        ) &&
        sr25519.verify(
          signedMessage(kind, transcript, mac),
          fromHex(evidence.messageSignature),
          fromHex(peer.identity.signingPublic),
        )
      );
    } catch {
      return false;
    }
  }

  openSealed(sealed: Uint8Array, purpose: string): Uint8Array | null {
    return openSealed(this.credential.agreement, sealed, purpose);
  }
}
