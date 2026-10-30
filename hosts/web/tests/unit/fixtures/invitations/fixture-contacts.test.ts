import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import {
  HostContacts,
  contactHandle,
} from "../../../../src/contacts/contacts.js";
import type { ChainDirectory } from "../../../../src/chat/nova/directory.js";
import { FixtureContacts } from "../../../../fixtures/invitations/fixture-contacts.js";
import {
  TEST_USERS,
  chatContactsOf,
  testUserAccount,
  testUserDirectory,
  testUserKeys,
  type TestUser,
} from "../../../../fixtures/invitations/test-users.js";
import { SUFFIX, handleKeyOf } from "../../../support/host-handles.js";
import { settle } from "../../../support/test-support.js";

const [ALICE, BOB, CAROL] = TEST_USERS as [TestUser, TestUser, TestUser];

function index(user: TestUser) {
  let answer: (handles: readonly HexString[]) => void = () => {};
  const handles = new HostContacts({
    list: () => chatContactsOf(user, SUFFIX),
    onChange: () => () => {},
    choose: async () => null,
    blocked: () => [],
    onSessionChange: () => () => {},
    removed: () => {},
    mint: () => new Promise((resolve) => (answer = resolve)),
  });
  return {
    handles,
    answer: () =>
      answer(
        chatContactsOf(user, SUFFIX).map((contact) =>
          contactHandle(handleKeyOf(user), contact.account),
        ),
      ),
  };
}

describe("fixture contacts", () => {
  test("name a person only once the host's index holds the core's answer, and say when that changes", async () => {
    const { handles, answer } = index(ALICE);
    const contacts = new FixtureContacts(ALICE, handles, {
      networkSuffix: SUFFIX,
      directory: testUserDirectory(SUFFIX),
    });
    await contacts.refresh();
    let told = 0;
    const stop = contacts.onChange(() => (told += 1));
    handles.followSession(testUserAccount(ALICE, SUFFIX));
    const bob = contacts.byAccount(testUserAccount(BOB, SUFFIX))!;
    expect(contacts.handleFor(bob)).toBeNull();

    answer();
    await settle();
    const handle = contacts.handleFor(bob)!;
    expect([told, contacts.resolve(handle)?.id]).toEqual([1, "bob"]);

    stop();
    handles.followSession(null);
    expect([told, contacts.resolve(handle)]).toEqual([1, null]);
  });

  test("a person is their identity account with the chat key published for it", async () => {
    const { handles } = index(ALICE);
    const contacts = new FixtureContacts(ALICE, handles, {
      networkSuffix: SUFFIX,
      directory: testUserDirectory(SUFFIX),
    });
    expect(contacts.byAccount(testUserAccount(BOB, SUFFIX))).toBeUndefined();
    await contacts.refresh();
    const account = testUserAccount(BOB, SUFFIX);
    const bob = contacts.byAccount(account)!;
    expect({ account: bob.account, identity: bob.identity }).toEqual({
      account,
      identity: {
        signingPublic: account,
        agreementPublic: `0x${Buffer.from(testUserKeys(BOB, SUFFIX).chatPublic).toString("hex")}`,
      },
    });
    expect(contacts.byIdentity(account)).toEqual(bob);
  });

  test("a person with no published key, or two different ones, can be named but not reached, and the host says why", async () => {
    const { handles, answer } = index(ALICE);
    handles.followSession(testUserAccount(ALICE, SUFFIX));
    answer();
    await settle();
    const fixture = testUserDirectory(SUFFIX);
    const bobAccount = testUserAccount(BOB, SUFFIX);
    const directory: ChainDirectory = {
      ...fixture,
      consumer: async (account) =>
        `0x${Buffer.from(account).toString("hex")}` === bobAccount
          ? null
          : fixture.consumer(account),
      names: async (account) =>
        `0x${Buffer.from(account).toString("hex")}` ===
        testUserAccount(CAROL, SUFFIX)
          ? {
              lite: { label: "carol", chatKey: new Uint8Array(32).fill(9) },
              full: null,
            }
          : null,
    };
    const logged: string[] = [];
    const contacts = new FixtureContacts(ALICE, handles, {
      networkSuffix: SUFFIX,
      directory,
      log: (line) => logged.push(line),
    });
    const bobHandle = {
      bytes: handles.handleFor(bobAccount)!,
    };
    expect(contacts.problem(bobHandle)).toBe("Bob's chat key is not read yet.");
    await contacts.refresh();
    expect([
      contacts.resolve(bobHandle),
      contacts.problem(bobHandle),
      contacts.byAccount(testUserAccount(CAROL, SUFFIX)),
    ]).toEqual([null, "Bob: it has no chat identity on chain.", undefined]);
    expect(logged.sort()).toEqual([
      "Cannot reach Bob: it has no chat identity on chain.",
      "Cannot reach Carol: Asset Hub and the People chain publish different chat keys.",
    ]);
    expect(contacts.nameOf(bobAccount)?.name).toBe("Bob");
    expect(contacts.problem({ bytes: `0x${"00".repeat(32)}` })).toBeNull();
  });
});
