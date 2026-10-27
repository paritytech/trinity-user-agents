import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import type { ProductContext } from "@parity/truapi-host";
import type { ChatContact } from "./chat-provider.js";
import {
  type DiscoveryView,
  type ListedUser,
  fixtureDirectory,
} from "./username-directory.js";
import {
  HostContacts,
  type PickedPerson,
  byRecent,
  contactHandle,
  lookupView,
  usernameLookup,
} from "./contacts.js";
import type { PromptLookupView } from "./prompt.js";

const product: ProductContext = {
  productId: "pay.paseo",
  executionKind: "App",
};
const KIM = `0x${"4b".repeat(32)}` as HexString;
const SAM = `0x${"5a".repeat(32)}` as HexString;
const KEY = `0x${"11".repeat(32)}` as HexString;

const contactPick = (account: HexString): PickedPerson => ({
  account,
  name: "a contact",
  found: false,
});
const foundPick = (account: HexString): PickedPerson => ({
  account,
  name: "found.01",
  found: true,
});

function setup(
  initial: ChatContact[],
  answer:
    | PickedPerson
    | null
    | (() => Promise<PickedPerson | null>) = contactPick(KIM),
) {
  let list = initial;
  let blocked: HexString[] = [];
  const listeners = new Set<() => void>();
  const sessionListeners = new Set<() => void>();
  const shown: {
    title: string;
    from?: string;
    names: string[];
    blocked: string[];
  }[] = [];
  let removed = 0;
  const contacts = new HostContacts({
    list: () => list,
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    choose: async (asker, offered, hidden) => {
      shown.push({
        ...asker,
        names: offered.map((contact) => contact.name),
        blocked: [...hidden],
      });
      return typeof answer === "function" ? answer() : answer;
    },
    blocked: () => blocked,
    onSessionChange: (listener) => {
      sessionListeners.add(listener);
      return () => sessionListeners.delete(listener);
    },
    removed: () => (removed += 1),
  });
  const change = (next: ChatContact[]) => {
    list = next;
    for (const listener of listeners) listener();
  };
  const block = (accounts: HexString[]) => {
    blocked = accounts;
    for (const listener of listeners) listener();
  };
  const newSession = () => {
    for (const listener of sessionListeners) listener();
  };
  const resolve = (account: HexString) =>
    contacts.platform
      .contacts({ handleKey: KEY, handles: [contactHandle(KEY, account)] })
      .then((matches) => matches.accounts[0]);
  return {
    contacts,
    shown,
    change,
    block,
    newSession,
    resolve,
    removed: () => removed,
  };
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
      {
        title: "Choose a contact",
        from: "pay.paseo",
        names: ["kim.01", "sam.01"],
        blocked: [],
      },
    ]);
  });

  test("answers Dismissed when the picker is closed", async () => {
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

  test("a contact row naming someone who is not a contact is not an answer", async () => {
    const { contacts } = setup(
      [{ account: KIM, name: "kim.01" }],
      contactPick(SAM),
    );
    expect(await contacts.platform.pickContact(product)).toEqual({
      tag: "Dismissed",
    });
  });

  test("a person found by username is picked, without becoming a contact, and their handle resolves after the core forgets it", async () => {
    const { contacts, change, resolve, removed } = setup(
      [{ account: KIM, name: "kim.01" }],
      foundPick(SAM),
    );
    expect(await contacts.platform.pickContact(product)).toEqual({
      tag: "Picked",
      value: { account: SAM },
    });
    // Kim leaving empties the core's cache; the host still answers for Sam.
    change([]);
    expect([removed(), await resolve(SAM), await resolve(KIM)]).toEqual([
      1,
      SAM,
      undefined,
    ]);
  });

  test("with no contacts the picker still opens, since anyone can be found by username", async () => {
    const { contacts, shown } = setup([], foundPick(SAM));
    expect(await contacts.platform.pickContact(product)).toEqual({
      tag: "Picked",
      value: { account: SAM },
    });
    expect(shown.map((entry) => entry.names)).toEqual([[]]);
  });

  test("the host's own pick names whom the user chose, without the product name, a handle or remembering them", async () => {
    const { contacts, shown, block, resolve } = setup(
      [{ account: KIM, name: "kim.01" }],
      foundPick(SAM),
    );
    block([`0x${"77".repeat(32)}` as HexString]);
    expect(await contacts.choose({ title: "New chat" })).toEqual(
      foundPick(SAM),
    );
    expect(shown).toEqual([
      {
        title: "New chat",
        names: ["kim.01"],
        blocked: [`0x${"77".repeat(32)}`],
      },
    ]);
    // Only a product's pick keeps a found person resolvable for its handle.
    expect(await resolve(SAM)).toBeUndefined();
  });

  test("a blocked person is not picked, and a picked person the user blocks stops resolving", async () => {
    const refused = setup([], foundPick(SAM));
    refused.block([SAM]);
    expect(await refused.contacts.platform.pickContact(product)).toEqual({
      tag: "Dismissed",
    });

    const later = setup([], foundPick(SAM));
    await later.contacts.platform.pickContact(product);
    later.block([SAM]);
    expect([later.removed(), await later.resolve(SAM)]).toEqual([1, undefined]);
    later.block([]);
    expect(await later.resolve(SAM)).toBeUndefined();
  });

  test("a new session forgets the people picked by username and drops a choice made across it", async () => {
    const { contacts, newSession, resolve, removed } = setup(
      [],
      foundPick(SAM),
    );
    await contacts.platform.pickContact(product);
    newSession();
    expect([removed(), await resolve(SAM)]).toEqual([1, undefined]);

    let release: (person: PickedPerson) => void = () => {};
    const across = setup(
      [],
      () => new Promise<PickedPerson | null>((done) => (release = done)),
    );
    const picked = across.contacts.platform.pickContact(product);
    await Promise.resolve();
    across.newSession();
    release(foundPick(SAM));
    expect(await picked).toEqual({ tag: "Dismissed" });
    expect(await across.resolve(SAM)).toBeUndefined();
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

  test("the picker lists contacts chatted with most recently first, then the rest as they come", () => {
    const contact = (name: string, lastActiveAt?: number) => ({
      account: `0x${name.repeat(32)}` as HexString,
      name,
      ...(lastActiveAt === undefined ? {} : { lastActiveAt }),
    });
    expect(
      byRecent([
        contact("aa"),
        contact("bb", 5),
        contact("cc"),
        contact("dd", 9),
      ]).map((entry) => entry.name),
    ).toEqual(["dd", "bb", "aa", "cc"]);
  });
});

describe("the picker's username group", () => {
  const found = [{ account: SAM, username: "sam.07" }];
  const plain = (view: PromptLookupView) => ({
    ...view,
    rows: view.rows.map(({ label, avatarSeed }) => ({ label, avatarSeed })),
  });

  test.each([
    [
      "is hidden before anything is typed",
      { tag: "idle" } as DiscoveryView,
      { visible: false, rows: [], enabled: true, status: null, more: false },
    ],
    [
      "says what a username looks like",
      { tag: "invalid" } as DiscoveryView,
      {
        visible: true,
        rows: [],
        enabled: true,
        status: "Usernames are letters and digits, like alice or alice.06.",
        more: false,
      },
    ],
    [
      "says it is searching",
      { tag: "searching", users: [] } as DiscoveryView,
      {
        visible: true,
        rows: [],
        enabled: true,
        status: "Searching usernames…",
        more: false,
      },
    ],
    [
      "says when no one matched",
      { tag: "found", users: [], more: false } as DiscoveryView,
      {
        visible: true,
        rows: [],
        enabled: true,
        status: "No one else by that username.",
        more: false,
      },
    ],
    [
      "lists people by username and offers the next page",
      { tag: "found", users: found, more: true } as DiscoveryView,
      {
        visible: true,
        rows: [{ label: "sam.07", avatarSeed: SAM }],
        enabled: true,
        status: null,
        more: true,
      },
    ],
    [
      "keeps what was found and says why the search failed",
      {
        tag: "failed",
        users: found,
        message: "Too many searches.",
      } as DiscoveryView,
      {
        visible: true,
        rows: [{ label: "sam.07", avatarSeed: SAM }],
        enabled: true,
        status: "Too many searches.",
        more: false,
      },
    ],
  ])("%s", (_, view, shown) => {
    expect(plain(lookupView(view, () => {}))).toEqual(shown);
  });

  /** Open the group over `directory`, type `text` and wait for its rows. */
  async function opened(
    directory: Parameters<typeof usernameLookup>[0],
    text: string,
    blocked: ReadonlySet<string> = new Set(),
  ) {
    const views: PromptLookupView[] = [];
    const answers: (PickedPerson | null)[] = [];
    const session = usernameLookup(
      directory,
      [{ account: KIM, name: "kim.01" }],
      blocked,
    ).open(
      (view) => views.push(view),
      (value) => answers.push(value),
    );
    session.type(text);
    await new Promise((resolve) => setTimeout(resolve, 350));
    return { session, views, answers, last: () => views.at(-1)! };
  }

  const users: ListedUser[] = [
    { account: KIM, username: "kim.01" },
    { account: SAM, username: "sam.07" },
  ];

  test("choosing a person checks them first, then answers with their account as found", async () => {
    let check: (problem: string | null) => void = () => {};
    let checks = 0;
    const directory = {
      ...fixtureDirectory("test", users),
      confirm: () => {
        checks += 1;
        return new Promise<string | null>((done) => (check = done));
      },
    };
    const { views, answers, last } = await opened(directory, "s");
    // The contact Kim is not listed again; Sam is.
    expect(last().rows.map((row) => row.label)).toEqual(["sam.07"]);
    last().rows[0].choose();
    expect([last().enabled, last().status, answers]).toEqual([
      false,
      "Checking sam.07 on chain…",
      [],
    ]);
    // A second tap while checking does nothing.
    views.at(-1)!.rows[0].choose();
    check(null);
    await Promise.resolve();
    expect([checks, answers]).toEqual([
      1,
      [{ account: SAM, name: "sam.07", found: true }],
    ]);
  });

  test("a blocked person is not listed among the people found", async () => {
    const { last } = await opened(
      fixtureDirectory("test", users),
      "s",
      new Set([SAM.toLowerCase()]),
    );
    expect([last().rows, last().status]).toEqual([
      [],
      "No one else by that username.",
    ]);
  });

  test("a person whose username names someone else on chain is not picked", async () => {
    const directory = {
      ...fixtureDirectory("test", users),
      confirm: async () => "On chain, sam.07 names another account.",
    };
    const { answers, last } = await opened(directory, "s");
    last().rows[0].choose();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect([answers, last().enabled, last().status]).toEqual([
      [],
      true,
      "On chain, sam.07 names another account.",
    ]);
  });

  test("a check that ends after the picker closed answers nothing", async () => {
    let check: (problem: string | null) => void = () => {};
    const directory = {
      ...fixtureDirectory("test", users),
      confirm: () => new Promise<string | null>((done) => (check = done)),
    };
    const { session, answers, last } = await opened(directory, "s");
    last().rows[0].choose();
    session.close();
    check(null);
    await Promise.resolve();
    expect(answers).toEqual([]);
  });
});
