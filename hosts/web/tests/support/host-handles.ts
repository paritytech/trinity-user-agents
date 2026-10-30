import type { HexString } from "@parity/truapi/scale";
import { HostContacts, contactHandle } from "../../src/contacts/contacts.js";
import { digest, toHex } from "../../src/invitations/sealing.js";
import { FixtureContacts } from "../../fixtures/invitations/fixture-contacts.js";
import {
  TEST_USERS,
  chatContactsOf,
  testUserAccount,
  testUserDirectory,
  type TestUser,
} from "../../fixtures/invitations/test-users.js";

/** The network suffix unit tests derive test users' identity accounts on. */
export const SUFFIX = "paseo";

/** A stand-in for one user's session handle key: the core derives its own from the session's root. */
export const handleKeyOf = (user: TestUser): HexString =>
  toHex(digest("test/contact-handle-key", user.id)) as HexString;

/**
 * The host's contacts for `user`, wired as `src/main.ts` wires them for a
 * test user, over a stand-in for the core's mint that hashes as the core
 * does under {@link handleKeyOf}. Unit tests only: the browser tests mint
 * with the core. Signed in already; the index fills once its mint answers.
 */
export function hostContactsOf(user: TestUser): HostContacts {
  const contacts = new HostContacts({
    list: () => chatContactsOf(user, SUFFIX),
    onChange: () => () => {},
    choose: async () => null,
    blocked: () => [],
    onSessionChange: () => () => {},
    removed: () => {},
    mint: async (accounts) =>
      accounts.map((account) => contactHandle(handleKeyOf(user), account)),
    named: () =>
      TEST_USERS.filter((other) => other.id !== user.id).map((other) =>
        testUserAccount(other, SUFFIX),
      ),
  });
  contacts.followSession(testUserAccount(user, SUFFIX));
  return contacts;
}

/** `user`'s fixture contacts over the fixture directory, with every chat key read. */
export async function fixtureContactsOf(
  user: TestUser,
  handles = hostContactsOf(user),
): Promise<FixtureContacts> {
  const contacts = new FixtureContacts(user, handles, {
    networkSuffix: SUFFIX,
    directory: testUserDirectory(SUFFIX),
  });
  await contacts.refresh();
  return contacts;
}

/** Wait until every index a test made has its mint's answer. */
export const indexed = (): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, 0));
