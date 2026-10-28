import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import type { ChainDirectory } from "../../../src/chat/nova/directory.js";
import { HostNames } from "../../../src/contacts/host-names.js";

const ADA = `0x${"ad".repeat(32)}`;
const NOBODY = `0x${"0b".repeat(32)}`;

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A chain that names Ada `ada.01` and no one else, and fails every read while it is down. */
function chain() {
  const reads: string[] = [];
  let down = false;
  const read = async (account: Uint8Array) => {
    reads.push(bytesToHex(account));
    if (down) throw new Error("the chain is not reachable");
  };
  const directory: ChainDirectory = {
    names: async (account) => {
      await read(account);
      return null;
    },
    consumer: async (account) => {
      await read(account);
      return bytesToHex(account) === ADA
        ? { liteUsername: "ada.01", fullUsername: null, chatKey: null }
        : null;
    },
    owners: async () => ({ assetHub: null, people: null }),
    hasAllowance: async () => false,
  };
  return {
    directory,
    reads: (account: string) => reads.filter((read) => read === account).length,
    setDown: (value: boolean) => (down = value),
  };
}

describe("the host's names", () => {
  test("a failed chain read is asked again later, and an account with no name is asked once", async () => {
    const fake = chain();
    const names = new HostNames();
    let changes = 0;
    names.onChange(() => (changes += 1));
    names.setSession({
      scope: "paseo:w1",
      record: () => null,
      directory: () => fake.directory,
    });

    fake.setDown(true);
    expect(names.name(ADA)).toBeNull();
    await settle();
    fake.setDown(false);
    expect(names.name(ADA)).toBeNull();
    await settle();
    expect(names.name(ADA)).toBe("ada.01");
    expect(changes).toBe(1);

    expect(names.name(NOBODY)).toBeNull();
    await settle();
    expect(names.name(NOBODY)).toBeNull();
    await settle();
    // One lookup reads both of its records, and only once.
    expect(fake.reads(NOBODY)).toBe(2);
  });
});
