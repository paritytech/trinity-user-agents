import type { PeerKeys } from "./binding.js";
import type { ContactRef } from "./contract.js";

/**
 * What the invitation service needs from the rest of the host. Each is a
 * PROPOSED operation with no official binding yet; in this repository the
 * dev tools implement them over made-up users (`src/dev-tools/`).
 */

export interface Product {
  id: string;
  name: string;
}

/** A person the host knows. Host only: no field reaches a product. */
export interface Person {
  id: string;
  name: string;
  /** Shown as the person's username, and seeds their avatar. */
  handle: string;
  keys: PeerKeys;
}

/**
 * PROPOSED host Contacts operations. Today `contacts.pick` returns a handle
 * the core minted, and the host cannot resolve it outside a transaction; this
 * port is the operation the invitation needs instead.
 */
export interface ContactService {
  /** Show the host's picker for `product`. The reference for whom the user chose, or null. */
  pick(product: Product): Promise<ContactRef | null>;
  /** Whom `contact` names for `product`, or null when it names nobody now. */
  resolve(product: Product, contact: ContactRef): Person | null;
  /** The contact with this signing key, to authenticate a sender. */
  bySigningKey(signingPublic: string): Person | undefined;
  /** The reference `product` knows `person` by. */
  refFor(product: Product, person: Person): ContactRef;
}

/**
 * How sealed invitations travel between hosts. One way only: there is no
 * receipt, answer or decline back to the sender.
 */
export interface InvitationDelivery {
  /** Start passing what arrives for this host to `receive`. */
  listen(receive: (sealed: string) => void): void;
  /**
   * Post `sealed` to `to`, under `id`, and post it again until
   * {@link withdraw} or `until`. The recipient's host keeps the first copy.
   */
  post(to: PeerKeys, id: string, sealed: string, until: number): void;
  /** Stop posting `id`. What was posted stays where it is until it lapses. */
  withdraw(id: string): void;
  close(): void;
}
