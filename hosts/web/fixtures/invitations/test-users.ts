import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "../../src/chat/provider.js";
import type { ChainDirectory } from "../../src/platform/chain-directory.js";
import { ChatKeys } from "../../src/invitations/chat-keys.js";
import type { Person } from "../../src/invitations/ports.js";
import {
  DEV_ACCOUNTS,
  devAccountEntropy,
  type DevAccount,
  type DevAccountId,
} from "../../src/platform/dev-accounts.js";

/**
 * DEV ONLY. Made-up users a tab can act as instead of a wallet, to try
 * features that need more than one person. Nothing about them is verified.
 *
 * Each is a built-in account ({@link DEV_ACCOUNTS}): its keys are the wallet
 * keys its public phrase derives, as for any wallet, so anyone who has the
 * phrase can act as it. A username and a contact list are made up. The
 * contacts stand in for the chat contacts a wallet's chat providers would
 * offer the host's picker, and name each user by their identity account on
 * the network, as chat names people.
 */
export interface TestUser extends DevAccount {
  /** Shown as the user's username. */
  handle: string;
  /** Ids of the test users in this user's contacts. */
  contacts: readonly DevAccountId[];
}

const FIXTURES: Record<DevAccountId, Pick<TestUser, "handle" | "contacts">> = {
  alice: { handle: "alice.example", contacts: ["bob", "carol"] },
  bob: { handle: "bob.example", contacts: ["alice"] },
  carol: { handle: "carol.example", contacts: ["alice"] },
};

export const TEST_USERS: readonly TestUser[] = DEV_ACCOUNTS.map((account) => ({
  ...account,
  ...FIXTURES[account.id],
}));

/** The entropy the core signs `user` in with. */
export function testUserEntropy(user: TestUser): Uint8Array {
  return devAccountEntropy(user);
}

const derived = new Map<string, ChatKeys>();

/** The wallet keys `user`'s phrase derives on the network with `networkSuffix`, as the core derives them. */
export function testUserKeys(user: TestUser, networkSuffix: string): ChatKeys {
  const key = `${user.id}:${networkSuffix}`;
  let keys = derived.get(key);
  if (keys === undefined) {
    keys = ChatKeys.fromEntropy(testUserEntropy(user), networkSuffix);
    derived.set(key, keys);
  }
  return keys;
}

/** `user`'s identity account on the network with `networkSuffix`. */
export const testUserAccount = (
  user: TestUser,
  networkSuffix: string,
): HexString =>
  bytesToHex(testUserKeys(user, networkSuffix).identity) as HexString;

/** `user` as another host knows them, once it read their published chat key. */
export function personOf(
  user: TestUser,
  networkSuffix: string,
  chatKey: Uint8Array,
): Person {
  const account = testUserAccount(user, networkSuffix);
  return {
    id: user.id,
    account,
    name: user.name,
    handle: user.handle,
    identity: { signingPublic: account, agreementPublic: bytesToHex(chatKey) },
  };
}

/** The test users in `user`'s contacts. */
export function contactsOf(user: TestUser): TestUser[] {
  return TEST_USERS.filter((other) => user.contacts.includes(other.id));
}

/** `user`'s contacts as the host's contact picker lists them. */
export function chatContactsOf(
  user: TestUser,
  networkSuffix: string,
): ChatContact[] {
  return contactsOf(user).map((other) => ({
    account: testUserAccount(other, networkSuffix),
    name: other.name,
  }));
}

/**
 * FIXTURE DIRECTORY, in place of the chain. The test users have no chain
 * records, so this answers as the People chain would if each had registered
 * its username and the chat key its phrase derives. It proves nothing about
 * who holds a key; a real host reads `chainDirectory` instead.
 */
export function testUserDirectory(networkSuffix: string): ChainDirectory {
  const byAccount = new Map(
    TEST_USERS.map((user) => [testUserAccount(user, networkSuffix), user]),
  );
  return {
    names: async () => null,
    consumer: async (account) => {
      const user = byAccount.get(bytesToHex(account) as HexString);
      return user === undefined
        ? null
        : {
            liteUsername: user.id,
            fullUsername: null,
            chatKey: testUserKeys(user, networkSuffix).chatPublic,
          };
    },
    owners: async () => ({ assetHub: null, people: null }),
    hasAllowance: async () => false,
  };
}

const TAB_KEY = "truapi-web-signing-host:dev:test-user";

/** The test user this tab acts as, or null for the normal wallet session. */
export function tabTestUser(storage: Storage): TestUser | null {
  const id = storage.getItem(TAB_KEY);
  return TEST_USERS.find((user) => user.id === id) ?? null;
}

export function setTabTestUser(storage: Storage, id: string | null): void {
  if (id === null) storage.removeItem(TAB_KEY);
  else storage.setItem(TAB_KEY, id);
}

/**
 * The version of each list a test user keeps. Each moved on when what it
 * holds stopped being readable: v2 with the host-owned request flow, then
 * every list with the wallet's own keys, whose requests name identity
 * accounts. Earlier lists are left as they are and no longer read. Contact
 * Chat's requests are in its own lists; `outbox` and `received` are no
 * longer written, and clearing a test user's invitations removes them too.
 */
export const PART_VERSION = {
  notifications: 2,
  outbox: 3,
  received: 3,
  "contact-chat-outbox": 2,
  "contact-chat-received": 2,
  "kettle-chat-outbox": 1,
  "kettle-chat-received": 1,
} as const;

/** Where a test user's data is kept, per network. */
export const testUserKey = (
  network: string,
  user: string,
  part: keyof typeof PART_VERSION,
  version: number = PART_VERSION[part],
): string =>
  `truapi-web-signing-host:network:${network}:test-user:${user}:${part}:v${version}`;

/** The host storage scope a test user's core session uses, apart from every wallet's. */
export const testUserScope = (user: TestUser): string => `test-user:${user.id}`;
