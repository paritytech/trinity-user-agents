import type { ContactHandle, ContactPickOutcome } from "./contract.js";

/**
 * What the request service needs from the rest of the host. Each is a
 * PROPOSED operation with no official binding yet; in this repository the
 * dev tools implement them over made-up users (`src/dev-tools/`).
 */

export interface Product {
  id: string;
  name: string;
}

/**
 * The public keys of one identity, in `0x` hex: an sr25519 signing key (its
 * account) and an X25519 key. A person may show a different, pseudonymous
 * identity to each contact; the host checks against the one it expects.
 */
export interface IdentityKeys {
  signingPublic: string;
  agreementPublic: string;
}

/** A person the host knows. Host only: no field reaches a product. */
export interface Person {
  id: string;
  name: string;
  /** Shown as the person's username, and seeds their avatar. */
  handle: string;
  /** The identity this person shows this host's user. */
  identity: IdentityKeys;
}

/**
 * PROPOSED host Contacts operations. `contacts.pick` returns a handle the
 * core minted, and the host cannot resolve it outside a transaction; this
 * port is what the request service needs instead.
 */
export interface ContactService {
  /** Show the host's picker for `product`, as `contacts.pick` does. */
  pick(product: Product): Promise<ContactPickOutcome>;
  /** Whom `contact` names, or null when it names nobody now. */
  resolve(contact: ContactHandle): Person | null;
  /** The contact whose identity has this signing key, to authenticate a sender. */
  byIdentity(signingPublic: string): Person | undefined;
  /** The handle a product knows `person` by. */
  handleFor(person: Person): ContactHandle;
}

export type EvidenceKind = "Request" | "Reply";

/** Authentication evidence: v3's `AuthEvidence`, in `0x` hex. */
export interface AuthEvidence {
  kind: EvidenceKind;
  /** 32 bytes. */
  identityMac: string;
  /** 64 bytes, sr25519. */
  messageSignature: string;
}

/**
 * PROPOSED. The host's identity credential: it authenticates as this user to
 * a contact, checks a contact's evidence, and opens what is sealed to the
 * user. The credential never leaves it; the request service only builds the
 * transcripts it covers. Which identity key a real host uses, and the exact
 * profile, are not decided; `src/dev-tools/fixture-identity.ts` is a
 * reference profile over made-up keys.
 */
export interface IdentityAuth {
  /** The identity this host's user shows `peer`. */
  identityFor(peer: Person): IdentityKeys;
  /** Evidence of `kind` over `transcript`, from `identityFor(peer)` to `peer`. */
  evidence(
    kind: EvidenceKind,
    transcript: Uint8Array,
    peer: Person,
  ): AuthEvidence;
  /** Whether `evidence` over `transcript` is from `peer` to `identityFor(peer)`. */
  check(
    kind: EvidenceKind,
    transcript: Uint8Array,
    evidence: AuthEvidence,
    peer: Person,
  ): boolean;
  /** Open what was sealed, for `purpose`, to one of this user's identities, or null. */
  openSealed(sealed: Uint8Array, purpose: string): Uint8Array | null;
}

/** What arrives for this host. */
export interface DeliveryReceiver {
  /** A sealed request addressed to this host. */
  request(sealed: string): void;
  /** A sealed reply on a return topic this host watches. */
  reply(topic: string, sealed: string): void;
}

/**
 * How sealed requests and replies travel between hosts. Requests go to a
 * recipient; replies go to the return topic named in the authenticated
 * request. Each post is repeated until withdrawn or `until`.
 */
export interface HostDelivery {
  listen(receiver: DeliveryReceiver): void;
  /** Post a request to `to`, under `id`. */
  postRequest(
    to: IdentityKeys,
    id: string,
    sealed: string,
    until: number,
  ): void;
  /** Post a reply on `topic`, under `id`. */
  postReply(topic: string, id: string, sealed: string, until: number): void;
  /** Hand this host what is posted on `topic`, now and as it arrives. */
  watch(topic: string): void;
  unwatch(topic: string): void;
  /** Stop posting `id`. What was posted stays until it lapses. */
  withdraw(id: string): void;
  close(): void;
}
