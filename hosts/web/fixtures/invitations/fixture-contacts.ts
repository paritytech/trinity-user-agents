import { hexToBytes } from "@parity/truapi/scale";
import type { HandleNames } from "../../src/contacts/contacts.js";
import type { ChainDirectory } from "../../src/chat/nova/directory.js";
import type { ContactHandle } from "../../src/invitations/contract.js";
import type { ContactService, Person } from "../../src/invitations/ports.js";
import { publishedChatKey } from "../../src/invitations/published-keys.js";
import {
  TEST_USERS,
  personOf,
  testUserAccount,
  type TestUser,
} from "./test-users.js";

/**
 * DEV ONLY. FIXTURE CONTACTS: the proposed host Contacts operations the
 * request service needs (`ContactService`), over the made-up test users.
 *
 * - The directory is every other test user, so an identity names a person
 *   here because the fixture says so. It is not their contact list: the
 *   host's contacts decide who a transaction may name.
 * - Each person is their identity account, with the chat key `directory`
 *   publishes for it. Until it is read, or when it is missing or the chains
 *   disagree, the person cannot be reached or authenticated.
 * - Handles are the core's, through the host's handle index.
 */
export class FixtureContacts implements ContactService {
  private readonly users: readonly TestUser[];
  /** Each user's published chat key, or why there is none, by lower-case account. */
  private readonly keys = new Map<
    string,
    { chatKey: Uint8Array } | { problem: string }
  >();
  private readonly listeners = new Set<() => void>();

  constructor(
    self: TestUser,
    private readonly handles: HandleNames,
    private readonly options: {
      networkSuffix: string;
      directory: ChainDirectory;
      log?(line: string): void;
    },
  ) {
    this.users = TEST_USERS.filter((user) => user.id !== self.id);
  }

  /** Read every person's published chat key again. */
  async refresh(): Promise<void> {
    await Promise.all(
      this.users.map(async (user) => {
        const account = this.accountOf(user);
        const published = await publishedChatKey(
          this.options.directory,
          hexToBytes(account),
        );
        if (published.ok)
          this.keys.set(account.toLowerCase(), { chatKey: published.chatKey });
        else {
          const problem = `${user.name}: ${published.reason}.`;
          this.keys.set(account.toLowerCase(), { problem });
          this.options.log?.(`Cannot reach ${problem}`);
        }
      }),
    );
    for (const listener of [...this.listeners]) listener();
  }

  resolve(contact: ContactHandle): Person | null {
    const user = this.named(contact);
    return user === undefined ? null : (this.person(user) ?? null);
  }

  problem(contact: ContactHandle): string | null {
    const user = this.named(contact);
    if (user === undefined) return null;
    const key = this.keys.get(this.accountOf(user).toLowerCase());
    if (key === undefined) return `${user.name}'s chat key is not read yet.`;
    return "problem" in key ? key.problem : null;
  }

  byIdentity(signingPublic: string): Person | undefined {
    return this.byAccount(signingPublic);
  }

  /** The person with identity account `account`, once their chat key is read. */
  byAccount(account: string): Person | undefined {
    const key = account.toLowerCase();
    const user = this.users.find(
      (other) => this.accountOf(other).toLowerCase() === key,
    );
    return user === undefined ? undefined : this.person(user);
  }

  /** Whom `account` names, with or without a chat key: enough to draw them. */
  nameOf(account: string): TestUser | undefined {
    const key = account.toLowerCase();
    return this.users.find(
      (other) => this.accountOf(other).toLowerCase() === key,
    );
  }

  handleFor(person: Person): ContactHandle | null {
    return this.handleForIdentity(person.account);
  }

  handleForIdentity(signingPublic: string): ContactHandle | null {
    if (this.nameOf(signingPublic) === undefined) return null;
    const bytes = this.handles.handleFor(signingPublic);
    return bytes === null ? null : { bytes };
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    const stopHandles = this.handles.onChange(listener);
    return () => {
      this.listeners.delete(listener);
      stopHandles();
    };
  }

  private named(contact: ContactHandle): TestUser | undefined {
    const account = this.handles.accountFor(contact.bytes);
    return account === null ? undefined : this.nameOf(account);
  }

  private person(user: TestUser): Person | undefined {
    const key = this.keys.get(this.accountOf(user).toLowerCase());
    return key === undefined || "problem" in key
      ? undefined
      : personOf(user, this.options.networkSuffix, key.chatKey);
  }

  private accountOf(user: TestUser): string {
    return testUserAccount(user, this.options.networkSuffix);
  }
}
