import type { HandleNames } from "../../src/contacts/contacts.js";
import type { ChainDirectory } from "../../src/chat/nova/directory.js";
import { DirectoryContacts } from "../../src/invitations/directory-contacts.js";
import { TEST_USERS, testUserAccount, type TestUser } from "./test-users.js";

/**
 * DEV ONLY. FIXTURE CONTACTS: the request service's people for a test user,
 * over the made-up test users.
 *
 * The people are every other test user, so an identity names a person here
 * because the fixture says so. It is not their contact list: the host's
 * contacts decide who a transaction may name. Their chat keys come from
 * `directory`, read up front by {@link refresh}.
 */
export class FixtureContacts extends DirectoryContacts {
  private readonly accounts: readonly string[];

  constructor(
    self: TestUser,
    handles: HandleNames,
    options: {
      networkSuffix: string;
      directory: ChainDirectory;
      log?(line: string): void;
    },
  ) {
    const users = TEST_USERS.filter((user) => user.id !== self.id);
    const byAccount = new Map(
      users.map((user) => [
        testUserAccount(user, options.networkSuffix).toLowerCase(),
        user,
      ]),
    );
    super({
      handles,
      directory: options.directory,
      person: (account) => {
        const user = byAccount.get(account.toLowerCase());
        return user === undefined
          ? undefined
          : { id: user.id, name: user.name, handle: user.handle };
      },
      ...(options.log === undefined ? {} : { log: options.log }),
    });
    this.accounts = [...byAccount.keys()];
  }

  /** Read every test user's published chat key again. */
  refresh(): Promise<void> {
    return this.readKeys(this.accounts);
  }
}
