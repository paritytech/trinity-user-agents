import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { DirectoryContacts } from "../../../src/invitations/directory-contacts.js";
import { memoryDirectory } from "../../support/nova-memory.js";

const BOB = new Uint8Array(32).fill(0xb0);
const BOB_ACCOUNT = bytesToHex(BOB);

/** People named as a wallet's Contact Chat names them: the host's name, then the chain's, then the account. */
function contacts(hostName: string | null) {
  return new DirectoryContacts({
    handles: {
      accountFor: () => null,
      handleFor: () => null,
      onChange: () => () => {},
    },
    directory: memoryDirectory([
      { label: "bob.10", identity: BOB, chatKey: new Uint8Array(32).fill(7) },
    ]),
    person: (account, published) => {
      const name = hostName ?? published ?? `${account.slice(0, 8)}…`;
      return { id: account, name, handle: name };
    },
  });
}

describe("people over the chain directory", () => {
  // A sender the host has no name for is shown by the username read with
  // their key, with no other read, not by their account.
  test("names a person by the username read with their chat key, after the host's own name", async () => {
    const unnamed = contacts(null);
    const before = unnamed.nameOf(BOB_ACCOUNT)?.name;
    await unnamed.learn({ identity: BOB_ACCOUNT });
    const named = contacts("Bob, as I saved him");
    await named.learn({ identity: BOB_ACCOUNT });

    expect([
      before,
      unnamed.nameOf(BOB_ACCOUNT)?.name,
      unnamed.byIdentity(BOB_ACCOUNT)?.name,
      named.byIdentity(BOB_ACCOUNT)?.name,
    ]).toEqual([
      `${BOB_ACCOUNT.slice(0, 8)}…`,
      "bob.10",
      "bob.10",
      "Bob, as I saved him",
    ]);
  });
});
