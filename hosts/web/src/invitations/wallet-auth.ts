import { blake2b } from "@noble/hashes/blake2.js";
import * as sr25519 from "@scure/sr25519";
import type {
  AuthEvidence,
  EvidenceKind,
  IdentityAuth,
  IdentityKeys,
  Person,
} from "./ports.js";
import { fromHex, toHex } from "../../bridges/primitives/bytes.js";
import { digest, openSealedWith } from "./sealing.js";

/**
 * PILOT, CLASSICAL. The host proof of the Establish Connection design
 * (`AuthEvidence`), made with the user's wallet keys. As the design asks, it
 * adapts native chat's identity authentication:
 *
 * - `identityMac`: BLAKE2b-256 keyed with the X25519 secret the two chat keys
 *   agree, over the kind and the transcript. Either side can make it, so it
 *   says "one of us"; the transcript's identity order and the signature say
 *   who.
 * - `messageSignature`: sr25519 by the identity account, over the kind, the
 *   transcript and the MAC.
 *
 * The keys are the wallet's own: the identity account and the chat key whose
 * public half the chain publishes for it. The bytes each covers are this
 * repository's reference profile, unchanged; the design leaves the exact
 * preimage, encoding and labels open, so these are not the production
 * profile. It shows one identity to every contact.
 */

/** The wallet keys the host proves its user with. The secrets stay inside. */
export interface WalletKeys {
  /** The identity account: sr25519, the sender identity peers check. */
  readonly identity: Uint8Array;
  /** The X25519 chat key published for the identity account. */
  readonly chatPublic: Uint8Array;
  sign(message: Uint8Array): Uint8Array;
  /** The raw X25519 secret the chat key shares with `peer`. */
  agree(peer: Uint8Array): Uint8Array;
}

const KIND_BYTE: Record<EvidenceKind, number> = { Request: 1, Reply: 2 };

/** The reference profile's label, kept so its bytes stay those reviewed; the production label is open. */
const SIGNATURE_DOMAIN = "truapi-web-host/dev-fixture/auth-signature/1";

function identityMac(
  shared: Uint8Array,
  kind: EvidenceKind,
  transcript: Uint8Array,
): Uint8Array {
  const message = new Uint8Array(1 + transcript.length);
  message[0] = KIND_BYTE[kind];
  message.set(transcript, 1);
  return blake2b(message, { key: shared, dkLen: 32 });
}

const signedMessage = (
  kind: EvidenceKind,
  transcript: Uint8Array,
  mac: Uint8Array,
): Uint8Array => digest(SIGNATURE_DOMAIN, kind, transcript, mac);

function equal(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1)
    difference |= left[index]! ^ right[index]!;
  return difference === 0;
}

export class WalletIdentityAuth implements IdentityAuth {
  private readonly identity: IdentityKeys;

  constructor(private readonly keys: WalletKeys) {
    this.identity = {
      signingPublic: toHex(keys.identity),
      agreementPublic: toHex(keys.chatPublic),
    };
  }

  identityFor(): IdentityKeys {
    return this.identity;
  }

  evidence(
    kind: EvidenceKind,
    transcript: Uint8Array,
    peer: Person,
  ): AuthEvidence {
    const mac = identityMac(this.shared(peer), kind, transcript);
    return {
      kind,
      identityMac: toHex(mac),
      messageSignature: toHex(
        this.keys.sign(signedMessage(kind, transcript, mac)),
      ),
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
        equal(mac, identityMac(this.shared(peer), kind, transcript)) &&
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
    return openSealedWith(
      (ephemeral) => this.keys.agree(ephemeral),
      this.keys.chatPublic,
      sealed,
      purpose,
    );
  }

  private shared(peer: Person): Uint8Array {
    return this.keys.agree(fromHex(peer.identity.agreementPublic));
  }
}
