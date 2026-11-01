import type { ContactHandle } from "./contract.js";

/**
 * What the request service needs from the rest of the host. Each is a
 * PROPOSED operation with no official binding yet; in this repository the
 * dev tools implement them over made-up users (`fixtures/invitations/`).
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
  /** The identity account: the core mints this person's contact handle from it. */
  account: string;
  name: string;
  /** Shown as the person's username, and seeds their avatar. */
  handle: string;
  /**
   * The identity this person shows this host's user: their identity account
   * and the chat key the chain publishes for it.
   */
  identity: IdentityKeys;
}

/**
 * PROPOSED host Contacts operations. A product picks a contact with
 * TrUAPI's `contacts.pick`; the host names that handle, and the people it
 * knows, through the handles the core mints for them. Naming someone is not
 * permission: a transaction still resolves a handle only for a contact.
 */
export interface ContactService {
  /** Whom `contact` names, with the keys to reach them, or null when it names nobody the host can reach now. */
  resolve(contact: ContactHandle): Person | null;
  /** Why the host cannot reach someone `contact` names, for the host's own log; null when it can, or names nobody. */
  problem(contact: ContactHandle): string | null;
  /** The person whose identity has this signing key, to authenticate a sender. */
  byIdentity(signingPublic: string): Person | undefined;
  /** The handle a product knows `person` by, or null when the host cannot name them now. */
  handleFor(person: Person): ContactHandle | null;
  /**
   * The handle a product knows the person with this identity by, for naming
   * a sender the host already authenticated; needs no current chat key.
   */
  handleForIdentity(signingPublic: string): ContactHandle | null;
  /** Calls back whenever {@link handleFor} may answer differently. Answers an unsubscribe. */
  onChange(listener: () => void): () => void;
  /**
   * Read the published key of the person with this identity, or of whom
   * `contact` names, when it is not read yet, so {@link byIdentity} and
   * {@link resolve} can answer. Absent where every key is read up front.
   */
  learn?(
    person: { identity: string } | { contact: ContactHandle },
  ): Promise<void>;
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
 * transcripts it covers. `wallet-auth.ts` is a pilot over the user's wallet
 * keys; the production profile is not decided.
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
  /** A sealed reply on a return route this host watches. */
  reply(route: string, sealed: string): void;
  /**
   * Whether the delivery can keep the post under `id` posted: a reason when
   * it cannot, such as an allowance it lacks, or null once it can again.
   */
  posting(id: string, problem: string | null): void;
}

/**
 * How sealed requests and replies travel between hosts. The delivery, not
 * the request service or a product, chooses both routes:
 *
 * - It reaches a request's recipient by their identity.
 * - It gives the sender a return route for each request. The route is
 *   opaque to the request service, which binds it into the authenticated
 *   request and keeps it with the pending request. The recipient's host
 *   gives it back to post the reply, so a reply reaches the sender and names
 *   the request it answers. What a route is, such as a topic or the sender's
 *   inbox with a tag, is the delivery's choice.
 *
 * A request stays posted until it is withdrawn: the delivery renews it on
 * its own cadence while this host runs. A reply stays posted until `until`.
 * Network retention is the delivery's; it says nothing of whether a request
 * may still be answered.
 */
export interface HostDelivery {
  listen(receiver: DeliveryReceiver): void;
  /**
   * The largest sealed post this delivery carries, in bytes. A request or
   * reply that seals larger is refused before anything is asked or kept.
   * Absent: no limit.
   */
  readonly maxPostBytes?: number;
  /** A new return route for one request, to this host. */
  returnRoute(): string;
  /** Whether `route` is one this delivery can post a reply on, to `requester`. */
  isReturnRoute(route: string, requester: IdentityKeys): boolean;
  /** Post a request to `to`, under `id`, until it is withdrawn. */
  postRequest(to: IdentityKeys, id: string, sealed: string): void;
  /** Post a reply on a return route, under `id`. */
  postReply(route: string, id: string, sealed: string, until: number): void;
  /** Hand this host what arrives on a return route of its own, now and as it arrives. */
  watch(route: string): void;
  unwatch(route: string): void;
  /** Stop posting and renewing `id`. What was posted stays until it lapses. */
  withdraw(id: string): void;
  close(): void;
}

/**
 * How the host gets a product's conversation account its Statement Store
 * allowance: a slot of the user's personhood, for the current allowance
 * period, for the product's own account at a raw derivation index. The core
 * derives the account and signs for the product; the host never posts with
 * it.
 */
export interface ConversationAllowance {
  /**
   * Allow `product`'s own account at `Raw(index)` to post for the current
   * period. Resolves with the account and when the period ends, in
   * milliseconds since the Unix epoch; rejects with why the host could not,
   * such as no free slot, a chain it cannot reach, or a tab that submits
   * nothing.
   */
  allow(
    product: string,
    index: Uint8Array,
  ): Promise<{ account: string; until: number }>;
}
