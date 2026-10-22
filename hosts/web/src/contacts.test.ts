import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import type { ProductContext } from "@parity/truapi-host";
import type { ChatContact } from "./chat-provider.js";
import { HostContacts, contactHandle } from "./contacts.js";

const product: ProductContext = {
  productId: "pay.paseo",
  executionKind: "App",
};
const KIM = `0x${"4b".repeat(32)}` as HexString;
const SAM = `0x${"5a".repeat(32)}` as HexString;
const KEY = `0x${"11".repeat(32)}` as HexString;

function setup(initial: ChatContact[], answer: HexString | null = KIM) {
  let list = initial;
  const listeners = new Set<() => void>();
  const shown: { productId: string; names: string[] }[] = [];
  let removed = 0;
  const contacts = new HostContacts({
    list: () => list,
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    choose: async (productId, offered) => {
      shown.push({ productId, names: offered.map((contact) => contact.name) });
      return answer;
    },
    removed: () => (removed += 1),
  });
  const change = (next: ChatContact[]) => {
    list = next;
    for (const listener of listeners) listener();
  };
  return { contacts, shown, change, removed: () => removed };
}

describe("host contacts", () => {
  test("hashes a handle as the core does", () => {
    // The vector from docs/design/contacts-pick-end-to-end.md.
    expect(contactHandle(KEY, `0x${"22".repeat(32)}`)).toBe(
      "0xd48c96fce9805f689b0bfa602feacdf3c7770d27e76c25d980eff0955e3714d2",
    );
  });

  test("answers each handle in order, and none for a handle no contact hashes to", async () => {
    const { contacts } = setup([
      { account: KIM, name: "kim.01" },
      { account: SAM, name: "sam.01" },
    ]);
    const made = `0x${"99".repeat(32)}` as HexString;
    expect(
      await contacts.platform.contacts({
        handleKey: KEY,
        handles: [contactHandle(KEY, SAM), made, contactHandle(KEY, KIM)],
      }),
    ).toEqual({ accounts: [SAM, undefined, KIM] });
  });

  test("names the product in the picker and returns the chosen account to the core", async () => {
    const { contacts, shown } = setup([
      { account: KIM, name: "kim.01" },
      { account: SAM, name: "sam.01" },
    ]);
    expect(await contacts.platform.pickContact(product)).toEqual({
      tag: "Picked",
      value: { account: KIM },
    });
    expect(shown).toEqual([
      { productId: "pay.paseo", names: ["kim.01", "sam.01"] },
    ]);
  });

  test("answers NoContacts without drawing a picker, and Dismissed when it is closed", async () => {
    const empty = setup([]);
    expect(await empty.contacts.platform.pickContact(product)).toEqual({
      tag: "NoContacts",
    });
    expect(empty.shown).toEqual([]);
    const closed = setup([{ account: KIM, name: "kim.01" }], null);
    expect(await closed.contacts.platform.pickContact(product)).toEqual({
      tag: "Dismissed",
    });
  });

  test("a contact that left while the picker was open is not an answer", async () => {
    const { contacts, change } = setup([{ account: KIM, name: "kim.01" }]);
    const picked = contacts.platform.pickContact(product);
    change([]);
    expect(await picked).toEqual({ tag: "Dismissed" });
  });

  test("tells the core when a contact leaves, and not when one is added", () => {
    const { change, removed } = setup([{ account: KIM, name: "kim.01" }]);
    change([
      { account: KIM, name: "kim.01" },
      { account: SAM, name: "sam.01" },
    ]);
    expect(removed()).toBe(0);
    change([{ account: SAM, name: "sam.01" }]);
    expect(removed()).toBe(1);
    // A session ending empties the list, which is a removal too.
    change([]);
    expect(removed()).toBe(2);
  });
});
