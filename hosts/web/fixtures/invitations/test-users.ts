import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "../../src/chat/provider.js";
import type { Person } from "../../src/invitations/ports.js";
import {
  DEV_ACCOUNTS,
  devAccountEntropy,
  type DevAccount,
  type DevAccountId,
} from "../../src/platform/dev-accounts.js";
import { digest } from "../../src/invitations/sealing.js";
import {
  fixtureCredential,
  publicIdentity,
  type FixtureCredential,
} from "./fixture-identity.js";

/**
 * DEV ONLY. Made-up users a tab can act as instead of a wallet, to try
 * features that need more than one person. Nothing about them is verified.
 *
 * Each is a built-in account ({@link DEV_ACCOUNTS}) with fixture data added:
 * the session is the account's own, and a handle, a contact account and a
 * contact list are made up. The contacts stand in for the chat contacts a
 * wallet's chat providers would offer the host's picker.
 */
export interface TestUser extends DevAccount {
  /** Shown as the user's username. */
  handle: string;
  /** The account the other test users' contact lists name this user by. */
  account: HexString;
  /** Ids of the test users in this user's contacts. */
  contacts: readonly DevAccountId[];
}

const FIXTURES: Record<
  DevAccountId,
  Pick<TestUser, "handle" | "account" | "contacts">
> = {
  alice: {
    handle: "alice.example",
    account: `0x${"a1".repeat(32)}`,
    contacts: ["bob", "carol"],
  },
  bob: {
    handle: "bob.example",
    account: `0x${"b0".repeat(32)}`,
    contacts: ["alice"],
  },
  carol: {
    handle: "carol.example",
    account: `0x${"c4".repeat(32)}`,
    contacts: ["alice"],
  },
};

export const TEST_USERS: readonly TestUser[] = DEV_ACCOUNTS.map((account) => ({
  ...account,
  ...FIXTURES[account.id],
}));

/** The entropy the core signs `user` in with. */
export function testUserEntropy(user: TestUser): Uint8Array {
  return devAccountEntropy(user);
}

/**
 * FIXTURE TRUST BOUNDARY. The identity credential of `user`: an sr25519
 * signing key and an X25519 key, both derived from the public user id, so
 * anyone can derive them. They let the request prototype authenticate and
 * seal for real, and prove nothing about a person. They are not the test
 * user's core session account. No real identity, key directory or lookup
 * exists.
 */
export function fixtureIdentity(user: TestUser): FixtureCredential {
  return fixtureCredential(
    digest("truapi-web-host/dev-fixture/identity-signing/1", user.id),
    digest("truapi-web-host/dev-fixture/identity-agreement/1", user.id),
  );
}

/** `user` as another host's fixture directory knows them. */
export function personOf(user: TestUser): Person {
  return {
    id: user.id,
    name: user.name,
    handle: user.handle,
    identity: publicIdentity(fixtureIdentity(user)),
  };
}

/** The test users in `user`'s contacts. */
export function contactsOf(user: TestUser): TestUser[] {
  return TEST_USERS.filter((other) => user.contacts.includes(other.id));
}

/** `user`'s contacts as the host's contact picker lists them. */
export function chatContactsOf(user: TestUser): ChatContact[] {
  return contactsOf(user).map(({ account, name }) => ({ account, name }));
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
 * The version of each list a test user keeps. The sent and received lists
 * moved to v2 with the host-owned request flow; v1 lists are left as they
 * are and no longer read.
 */
const PART_VERSION = { notifications: 1, outbox: 2, received: 2 } as const;

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
