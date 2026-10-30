import type { HexString } from "@parity/truapi/scale";
import { HostContacts, contactHandle } from "../../src/contacts/contacts.js";
import { digest, toHex } from "../../src/invitations/sealing.js";
import {
  TEST_USERS,
  chatContactsOf,
  type TestUser,
} from "../../fixtures/invitations/test-users.js";

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
    list: () => chatContactsOf(user),
    onChange: () => () => {},
    choose: async () => null,
    blocked: () => [],
    onSessionChange: () => () => {},
    removed: () => {},
    mint: async (accounts) =>
      accounts.map((account) => contactHandle(handleKeyOf(user), account)),
    named: () =>
      TEST_USERS.filter((other) => other.id !== user.id).map(
        (other) => other.account,
      ),
  });
  contacts.followSession(user.account);
  return contacts;
}

/** Wait until every index a test made has its mint's answer. */
export const indexed = (): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, 0));
