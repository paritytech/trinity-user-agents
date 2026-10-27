import type { ContactRef } from "../invitations/contract.js";
import type { ContactService, Person, Product } from "../invitations/ports.js";
import { digest, toHex } from "../invitations/sealing.js";
import {
  TEST_USERS,
  fixtureHostKeys,
  personOf,
  type TestUser,
} from "./test-users.js";

/**
 * DEV ONLY. FIXTURE CONTACTS: the proposed host Contacts operations the
 * invitation needs (`ContactService`), over the made-up test users.
 *
 * - `pick` opens the host's own contact picker, the one `contacts.pick`
 *   opens, and answers a reference for whom the user chose.
 * - A reference is `fixture-contact:` and 16 bytes keyed with this host's
 *   secret, per product and person. It stands in for the core's contact
 *   handle, which the host cannot resolve today outside a transaction. It is
 *   not that handle and makes no claim about how the core derives one.
 * - The directory is every other test user with their fixture keys, so a
 *   signing key names a person here because the fixture says so.
 */

/** Whom the host's picker answered: the account the user chose. */
export type PickContact = (asker: {
  title: string;
  from: string;
}) => Promise<{ account: string } | null>;

export const FIXTURE_REF_PREFIX = "fixture-contact:";

export class FixtureContacts implements ContactService {
  private readonly secret: Uint8Array;
  private readonly people: Person[];

  constructor(
    self: TestUser,
    private readonly choose: PickContact,
    private readonly log: (line: string) => void = () => {},
  ) {
    this.secret = fixtureHostKeys(self).inbox.secret;
    this.people = TEST_USERS.filter((user) => user.id !== self.id).map(
      personOf,
    );
  }

  async pick(product: Product): Promise<ContactRef | null> {
    const chosen = await this.choose({
      title: "Choose a contact",
      from: product.name,
    });
    if (chosen === null) return null;
    const user = TEST_USERS.find(
      (other) => other.account.toLowerCase() === chosen.account.toLowerCase(),
    );
    const person = this.people.find((other) => other.id === user?.id);
    if (person === undefined) {
      this.log("The person chosen is not a test user; no reference is given.");
      return null;
    }
    this.log(
      `${product.name} got a fixture contact reference for the person chosen, not a name, account or key.`,
    );
    return this.refFor(product, person);
  }

  resolve(product: Product, contact: ContactRef): Person | null {
    return (
      this.people.find((person) => this.refFor(product, person) === contact) ??
      null
    );
  }

  bySigningKey(signingPublic: string): Person | undefined {
    const key = signingPublic.toLowerCase();
    return this.people.find((person) => person.keys.signingPublic === key);
  }

  refFor(product: Product, person: Person): ContactRef {
    const bytes = digest(
      "truapi-web-host/dev-fixture/contact-ref/0",
      this.secret,
      product.id,
      person.id,
    );
    return `${FIXTURE_REF_PREFIX}${toHex(bytes.subarray(0, 16))}`;
  }
}
