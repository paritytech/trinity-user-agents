import { describe, expect, test } from "bun:test";
import { MemoryStorage } from "../../support/test-support.js";
import {
  WalletContacts,
  namesOf,
  readWalletRecord,
  walletRecordKey,
} from "../../../src/contacts/wallet-contacts.js";

const ADA = `0x${"aa".repeat(32)}` as const;
const BEN = `0x${"bb".repeat(32)}` as const;
const CY = `0x${"cc".repeat(32)}` as const;
const DEE = `0x${"dd".repeat(32)}` as const;
const EVE = `0x${"ee".repeat(32)}` as const;
const FAY = `0x${"ff".repeat(32)}` as const;

/** A contact as the saved chat record holds one, with the fields the host does not read. */
const contact = (
  account: string,
  name: string | null,
  extra: Record<string, unknown> = {},
) => ({
  account,
  chatKey: `0x${"01".repeat(32)}`,
  name,
  state: "active",
  messages: [],
  outbound: null,
  expiry: { request: null, response: null },
  acknowledged: null,
  ...extra,
});

const RECORD = JSON.stringify({
  contacts: [
    contact(ADA, "Ada", { messages: [{ at: 5 }, { at: 9 }] }),
    contact(BEN, "Ben", { blocked: true }),
    contact(CY, null, { state: "requested" }),
    contact(DEE, null, { blocked: false }),
  ],
  requests: [{ peer: EVE, name: "Eve" }],
  declined: [],
  left: [contact(CY, "Cy"), contact(FAY, "Fay")],
});

function open(offered: boolean, raw: string | null = RECORD) {
  const storage = new MemoryStorage();
  const key = walletRecordKey("paseo", "w1");
  if (raw !== null) storage.setItem(key, raw);
  const contacts = new WalletContacts(storage);
  contacts.setSession({
    key,
    scope: `${key}:${offered}`,
    offered: () => offered,
  });
  return { storage, contacts };
}

describe("the wallet's saved contacts", () => {
  // The picker offers these to products, and a transaction pays only someone
  // the picker offers: a person not accepted, or blocked, must not be one.
  test("are the accepted people not blocked or left, in the record's order, named or shortened", () => {
    expect(open(true).contacts.contacts()).toEqual([
      { account: ADA, name: "Ada", lastActiveAt: 9 },
      { account: DEE, name: "0xdddddd…dddddd" },
    ]);
  });

  test("are not offered while the session does not allow them; blocked people are refused either way", () => {
    const { contacts } = open(false);
    expect([
      contacts.contacts(),
      contacts.blocked(),
      contacts.blockedPeople(),
    ]).toEqual([[], [BEN], []]);
  });

  // Unblocking is the one write: it must keep everything else the record holds.
  test("unblocking clears only that person's block, keeps every other field, and offers them again", () => {
    const { storage, contacts } = open(true);
    let changes = 0;
    contacts.onChange(() => (changes += 1));
    expect(contacts.blockedPeople()).toEqual([{ account: BEN, name: "Ben" }]);
    const key = walletRecordKey("paseo", "w1");
    const expected = JSON.parse(RECORD) as {
      contacts: Record<string, unknown>[];
    };
    delete expected.contacts[1]!.blocked;

    contacts.unblock(BEN.toUpperCase().replace("0X", "0x"));
    expect([
      JSON.parse(storage.getItem(key)!),
      contacts.blocked(),
      contacts.contacts().map((person) => person.account),
      changes,
    ]).toEqual([expected, [], [ADA, BEN, DEE], 1]);

    // Someone not blocked, or not in the record, changes nothing.
    const kept = storage.getItem(key);
    contacts.unblock(ADA);
    contacts.unblock(EVE);
    expect([storage.getItem(key), changes]).toEqual([kept, 1]);
  });

  // Blocking writes what the host's chat wrote, and only that, so a record it
  // wrote keeps reading the same way.
  test("blocking sets only that contact's block, keeps every other field, and takes them out of the contacts", () => {
    const { storage, contacts } = open(true);
    let changes = 0;
    contacts.onChange(() => (changes += 1));
    const key = walletRecordKey("paseo", "w1");
    expect(contacts.blockable().map((person) => person.account)).toEqual([
      ADA,
      CY,
      DEE,
    ]);
    expect([contacts.canBlock(ADA), contacts.canBlock(BEN)]).toEqual([
      true,
      false,
    ]);
    const expected = JSON.parse(RECORD) as {
      contacts: Record<string, unknown>[];
    };
    expected.contacts[0]!.blocked = true;

    contacts.block(ADA);
    expect([
      JSON.parse(storage.getItem(key)!),
      contacts.blocked(),
      contacts.contacts().map((person) => person.account),
      changes,
    ]).toEqual([expected, [ADA, BEN], [DEE], 1]);

    // Blocked again, or not a contact of the record (a person who asked, or a stranger), changes nothing.
    const kept = storage.getItem(key);
    contacts.block(ADA);
    contacts.block(EVE);
    contacts.block(`0x${"99".repeat(32)}`);
    expect([storage.getItem(key), changes]).toEqual([kept, 1]);

    contacts.unblock(ADA);
    expect(storage.getItem(key)).toBe(JSON.stringify(JSON.parse(RECORD)));
  });

  test("a record that does not read, or holds no contacts list, is never written", () => {
    for (const raw of [
      "not a record",
      "null",
      JSON.stringify({ requests: [] }),
    ]) {
      const { storage, contacts } = open(true, raw);
      contacts.block(ADA);
      contacts.unblock(ADA);
      expect(storage.getItem(walletRecordKey("paseo", "w1"))).toBe(raw);
    }
  });

  test("blocking and unblocking are refused while the session does not allow them", () => {
    const { storage, contacts } = open(false);
    contacts.block(ADA);
    contacts.unblock(BEN);
    expect([
      storage.getItem(walletRecordKey("paseo", "w1")),
      contacts.canBlock(ADA),
      contacts.blockable(),
    ]).toEqual([RECORD, false, []]);
  });

  test("end with the session, and a new session tells its listeners", () => {
    const { contacts } = open(true);
    let changes = 0;
    contacts.onChange(() => (changes += 1));
    contacts.setSession(null);
    expect([contacts.contacts(), contacts.blocked(), changes]).toEqual([
      [],
      [],
      1,
    ]);
  });

  test("reading writes nothing, and a record that does not read is empty", () => {
    const { storage, contacts } = open(true, "not a record");
    expect([contacts.contacts(), contacts.blocked()]).toEqual([[], []]);
    expect(storage.length).toBe(1);
    expect(readWalletRecord(null)).toEqual(readWalletRecord("{}"));
  });

  test("name contacts, people left and people who asked, by lower-case account", () => {
    expect(Object.fromEntries(namesOf(readWalletRecord(RECORD)))).toEqual({
      [ADA]: "Ada",
      [BEN]: "Ben",
      [CY]: "Cy",
      [EVE]: "Eve",
      [FAY]: "Fay",
    });
  });
});
