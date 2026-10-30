import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import {
  HostContacts,
  contactHandle,
} from "../../../../src/contacts/contacts.js";
import { FixtureContacts } from "../../../../fixtures/invitations/fixture-contacts.js";
import {
  TEST_USERS,
  chatContactsOf,
  personOf,
  type TestUser,
} from "../../../../fixtures/invitations/test-users.js";
import { handleKeyOf } from "../../../support/host-handles.js";
import { settle } from "../../../support/test-support.js";

const [ALICE, BOB] = TEST_USERS as [TestUser, TestUser];

describe("fixture contacts", () => {
  test("name a person only once the host's index holds the core's answer, and say when that changes", async () => {
    let answer: (handles: readonly HexString[]) => void = () => {};
    const index = new HostContacts({
      list: () => chatContactsOf(ALICE),
      onChange: () => () => {},
      choose: async () => null,
      blocked: () => [],
      onSessionChange: () => () => {},
      removed: () => {},
      mint: () => new Promise((resolve) => (answer = resolve)),
    });
    const contacts = new FixtureContacts(ALICE, index);
    let told = 0;
    const stop = contacts.onChange(() => (told += 1));
    index.followSession(ALICE.account);
    expect(contacts.handleFor(personOf(BOB))).toBeNull();

    answer(
      chatContactsOf(ALICE).map((contact) =>
        contactHandle(handleKeyOf(ALICE), contact.account),
      ),
    );
    await settle();
    const handle = contacts.handleFor(personOf(BOB))!;
    expect([told, contacts.resolve(handle)?.id]).toEqual([1, "bob"]);

    stop();
    index.followSession(null);
    expect([told, contacts.resolve(handle)]).toEqual([1, null]);
  });
});
