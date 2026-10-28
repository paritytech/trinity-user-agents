import type {
  ContactHandle,
  ContactPickOutcome,
} from "../../src/invitations/contract.js";
import type {
  ContactService,
  Person,
  Product,
} from "../../src/invitations/ports.js";
import { digest, toHex } from "../../src/invitations/sealing.js";
import { TEST_USERS, personOf, type TestUser } from "./test-users.js";

/**
 * DEV ONLY. FIXTURE CONTACTS: the proposed host Contacts operations the
 * request service needs (`ContactService`), over the made-up test users.
 *
 * - `pick` opens the host's own contact picker, the one `contacts.pick`
 *   opens, and answers as `contacts.pick` does: `Picked` with a
 *   `ContactHandle`, or `Dismissed`.
 * - The handle is 32 bytes keyed with a secret of this test user over the
 *   person, so it names the person the same way in every product, as the
 *   core's handle does. It is not the core's handle: the core derives that
 *   from a key the host does not hold, and resolves it only for a
 *   transaction.
 * - The directory is every other test user with their fixture identity, so an
 *   identity names a person here because the fixture says so.
 */

/** Whom the host's picker answered: the account the user chose. */
export type PickContact = (asker: {
  title: string;
  from: string;
}) => Promise<{ account: string } | null>;

export class FixtureContacts implements ContactService {
  private readonly key: Uint8Array;
  private readonly people: Person[];

  constructor(
    self: TestUser,
    private readonly choose: PickContact,
    private readonly log: (line: string) => void = () => {},
  ) {
    this.key = digest(
      "truapi-web-host/dev-fixture/contact-handle-key/1",
      self.id,
    );
    this.people = TEST_USERS.filter((user) => user.id !== self.id).map(
      personOf,
    );
  }

  async pick(product: Product): Promise<ContactPickOutcome> {
    const chosen = await this.choose({
      title: "Choose a contact",
      from: product.name,
    });
    if (chosen === null) return { tag: "Dismissed" };
    const user = TEST_USERS.find(
      (other) => other.account.toLowerCase() === chosen.account.toLowerCase(),
    );
    const person = this.people.find((other) => other.id === user?.id);
    if (person === undefined) {
      this.log("The person chosen is not a test user; no handle is given.");
      return { tag: "Dismissed" };
    }
    this.log(
      `${product.name} got a fixture contact handle for the person chosen, not a name, account or key.`,
    );
    return { tag: "Picked", value: { handle: this.handleFor(person) } };
  }

  resolve(contact: ContactHandle): Person | null {
    const bytes = contact.bytes.toLowerCase();
    return (
      this.people.find((person) => this.handleFor(person).bytes === bytes) ??
      null
    );
  }

  byIdentity(signingPublic: string): Person | undefined {
    const key = signingPublic.toLowerCase();
    return this.people.find((person) => person.identity.signingPublic === key);
  }

  handleFor(person: Person): ContactHandle {
    return {
      bytes: toHex(
        digest(
          "truapi-web-host/dev-fixture/contact-handle/1",
          this.key,
          person.id,
        ),
      ) as ContactHandle["bytes"],
    };
  }
}
