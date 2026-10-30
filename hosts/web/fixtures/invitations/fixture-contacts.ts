import type { HandleNames } from "../../src/contacts/contacts.js";
import type { ContactHandle } from "../../src/invitations/contract.js";
import type { ContactService, Person } from "../../src/invitations/ports.js";
import { TEST_USERS, personOf, type TestUser } from "./test-users.js";

/**
 * DEV ONLY. FIXTURE CONTACTS: the proposed host Contacts operations the
 * request service needs (`ContactService`), over the made-up test users.
 *
 * - The directory is every other test user with their fixture identity, so an
 *   identity names a person here because the fixture says so. It is not
 *   their contact list: the host's contacts decide who a transaction may
 *   name.
 * - Handles are the core's: the host's handle index (`HandleNames`) holds
 *   what the core minted for each person's account under this user's
 *   session, the same handle `contacts.pick` gives every product. A handle
 *   the index does not hold names nobody.
 */
export class FixtureContacts implements ContactService {
  private readonly people: Person[];

  constructor(
    self: TestUser,
    private readonly handles: HandleNames,
  ) {
    this.people = TEST_USERS.filter((user) => user.id !== self.id).map(
      personOf,
    );
  }

  resolve(contact: ContactHandle): Person | null {
    const account = this.handles.accountFor(contact.bytes);
    return account === null ? null : (this.byAccount(account) ?? null);
  }

  byIdentity(signingPublic: string): Person | undefined {
    const key = signingPublic.toLowerCase();
    return this.people.find((person) => person.identity.signingPublic === key);
  }

  byAccount(account: string): Person | undefined {
    const key = account.toLowerCase();
    return this.people.find((person) => person.account.toLowerCase() === key);
  }

  handleFor(person: Person): ContactHandle | null {
    const bytes = this.handles.handleFor(person.account);
    return bytes === null ? null : { bytes };
  }

  onChange(listener: () => void): () => void {
    return this.handles.onChange(listener);
  }
}
