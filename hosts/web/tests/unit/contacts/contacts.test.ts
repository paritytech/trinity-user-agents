import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import type { ProductContext } from "@parity/truapi-host";
import type { ChatContact } from "../../../src/chat/provider.js";
import {
  type DiscoveryView,
  type ListedUser,
  fixtureDirectory,
} from "../../../src/contacts/username-directory.js";
import {
  HostContacts,
  MINT_RETRY_MS,
  type PickedPerson,
  byRecent,
  contactHandle,
  lookupView,
  usernameLookup,
} from "../../../src/contacts/contacts.js";
import type { PromptLookupView } from "../../../src/app/prompt.js";
import { MemoryStorage, settle } from "../../support/test-support.js";
import {
  WalletContacts,
  walletRecordKey,
} from "../../../src/contacts/wallet-contacts.js";

const product: ProductContext = {
  productId: "pay.paseo",
  executionKind: "App",
};
const KIM = `0x${"4b".repeat(32)}` as HexString;
const SAM = `0x${"5a".repeat(32)}` as HexString;
const KEY = `0x${"11".repeat(32)}` as HexString;
const OTHER_KEY = `0x${"12".repeat(32)}` as HexString;
const EVE = `0x${"e5".repeat(32)}` as HexString;

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
  let named: HexString[] = [];
  /** The key the core's session mints under now, or null with no session. */
  let sessionKey: HexString | null = KEY;
  /** How many mints from now on fail; -1 is all of them. */
  let failing = 0;
  /** Retries waiting to run, with the delay each asked for. */
  const timers: { delay: number; task: () => void; cancelled: boolean }[] = [];
  const mints: {
    accounts: readonly HexString[];
    answer: () => void;
  }[] = [];
  const logged: string[] = [];
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
    // The core answers under the session it has when it is asked.
    mint: (accounts) => {
      const key = sessionKey;
      const fail = failing !== 0;
      if (failing > 0) failing -= 1;
      return new Promise((resolve, reject) =>
        mints.push({
          accounts,
          answer: () =>
            fail
              ? reject(new Error("no core"))
              : resolve(
                  key === null
                    ? undefined
                    : accounts.map((account) => contactHandle(key, account)),
                ),
        }),
      );
    },
    named: () => named,
    schedule: (task, delay) => {
      const timer = { delay, task, cancelled: false };
      timers.push(timer);
      return () => (timer.cancelled = true);
    },
    log: (line) => logged.push(line),
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
    mints,
    logged,
    /** Answer every mint asked so far, one at a time, and let the index take each. */
    answerMints: async (
      order: "oldest first" | "newest first" = "oldest first",
    ) => {
      const asked = mints.splice(0);
      if (order === "newest first") asked.reverse();
      for (const mint of asked) {
        mint.answer();
        await settle();
      }
    },
    setNamed: (accounts: HexString[]) => (named = accounts),
    setSession: (key: HexString | null) => (sessionKey = key),
    failMints: (count = -1) => (failing = count),
    timers,
    /** Run the retries that wait and are not cancelled, and let what they ask settle. */
    runTimers: async () => {
      for (const timer of timers.splice(0)) if (!timer.cancelled) timer.task();
      await settle();
    },
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

describe("the host's names for contact handles", () => {
  const contactsOf = () => [
    { account: KIM, name: "kim.01" },
    { account: SAM, name: "sam.01" },
  ];

  test("names the handles the core minted for its contacts and the people it knows, and resolves only the contacts", async () => {
    const host = setup(contactsOf());
    host.setNamed([EVE, KIM]);
    host.contacts.followSession("alice");
    expect(host.mints.map((mint) => mint.accounts)).toEqual([[KIM, SAM, EVE]]);
    await host.answerMints();
    expect([
      host.contacts.accountFor(contactHandle(KEY, KIM).toUpperCase()),
      host.contacts.handleFor(EVE),
      host.contacts.accountFor(contactHandle(OTHER_KEY, KIM)),
    ]).toEqual([KIM, contactHandle(KEY, EVE), null]);
    // Naming is not paying: a transaction resolves the contacts alone.
    expect([await host.resolve(KIM), await host.resolve(EVE)]).toEqual([
      KIM,
      undefined,
    ]);
  });

  test("drops an answer a new session overtook, so no handle names anyone across sessions", async () => {
    const host = setup(contactsOf());
    host.contacts.followSession("alice");
    host.setSession(OTHER_KEY);
    host.contacts.followSession("bob");
    // The old session's answer arrives last, and must not take the index back.
    await host.answerMints("newest first");
    expect([
      host.contacts.handleFor(KIM),
      host.contacts.accountFor(contactHandle(KEY, KIM)),
    ]).toEqual([contactHandle(OTHER_KEY, KIM), null]);

    host.setSession(null);
    host.contacts.followSession(null);
    expect(host.contacts.handleFor(KIM)).toBeNull();
    await host.answerMints();
    expect(host.contacts.handleFor(KIM)).toBeNull();
  });

  test("forgets a person removed or blocked at once, before the core answers again", async () => {
    const host = setup(contactsOf());
    host.contacts.followSession("alice");
    await host.answerMints();
    let told = 0;
    host.contacts.onChange(() => (told += 1));
    host.change([{ account: SAM, name: "sam.01" }]);
    expect([host.contacts.handleFor(KIM), told]).toEqual([null, 1]);
    host.block([SAM]);
    expect(host.contacts.handleFor(SAM)).toBeNull();
    await host.answerMints();
    expect([
      host.contacts.handleFor(KIM),
      host.contacts.handleFor(SAM),
    ]).toEqual([null, null]);
  });

  // A block must reach the core at once: a handle a product holds for the
  // person stops resolving, so no transaction pays them.
  test("blocking a saved contact tells the core, and their handle no longer resolves", async () => {
    const storage = new MemoryStorage();
    const key = walletRecordKey("paseo", "w1");
    storage.setItem(
      key,
      JSON.stringify({
        contacts: [{ account: KIM, name: "kim.01", state: "active" }],
        requests: [],
      }),
    );
    const saved = new WalletContacts(storage);
    saved.setSession({ key, scope: key, offered: () => true });
    let removed = 0;
    const contacts = new HostContacts({
      list: () => saved.contacts(),
      onChange: (listener) => saved.onChange(listener),
      choose: async () => null,
      blocked: () => saved.blocked(),
      onSessionChange: () => () => {},
      removed: () => (removed += 1),
      mint: async (accounts) =>
        accounts.map((account) => contactHandle(KEY, account)),
    });
    const resolve = () =>
      contacts.platform
        .contacts({ handleKey: KEY, handles: [contactHandle(KEY, KIM)] })
        .then((matches) => matches.accounts[0]);
    expect([await resolve(), removed]).toEqual([KIM, 0]);

    saved.block(KIM);
    expect([await resolve(), removed]).toEqual([undefined, 1]);
    saved.unblock(KIM);
    expect(await resolve()).toBe(KIM);
  });

  test("a new chat session, or a change that names no one new, asks the core again only when it must", async () => {
    const host = setup(contactsOf());
    host.contacts.followSession("alice");
    await host.answerMints();
    host.change(contactsOf());
    expect(host.mints).toEqual([]);
    host.newSession();
    expect(host.contacts.handleFor(KIM)).toBeNull();
    await host.answerMints();
    expect(host.contacts.handleFor(KIM)).toBe(contactHandle(KEY, KIM));
  });

  test("a first mint still on its way names nobody yet, then everyone it answered for", async () => {
    const host = setup(contactsOf());
    let told = 0;
    host.contacts.onChange(() => (told += 1));
    host.contacts.followSession("alice");
    expect([host.contacts.status(), host.contacts.handleFor(KIM)]).toEqual([
      "pending",
      null,
    ]);
    await host.answerMints();
    expect([
      host.contacts.status(),
      host.contacts.handleFor(KIM),
      told,
    ]).toEqual(["ready", contactHandle(KEY, KIM), 1]);
  });

  test("a mint that failed is tried again after a wait, and the index fills once it works", async () => {
    const host = setup(contactsOf());
    host.failMints(1);
    host.contacts.followSession("alice");
    await host.answerMints();
    expect([host.contacts.status(), host.timers.map((t) => t.delay)]).toEqual([
      "pending",
      [MINT_RETRY_MS[0]],
    ]);
    await host.runTimers();
    await host.answerMints();
    expect([host.contacts.status(), host.contacts.handleFor(KIM)]).toEqual([
      "ready",
      contactHandle(KEY, KIM),
    ]);
    expect([host.timers, host.logged]).toEqual([[], []]);
  });

  test("a signed-in core that answers no session is tried again too, as a core not there yet", async () => {
    const host = setup(contactsOf());
    host.setSession(null);
    host.contacts.followSession("alice");
    await host.answerMints();
    expect(host.timers).toHaveLength(1);
    host.setSession(KEY);
    await host.runTimers();
    await host.answerMints();
    expect(host.contacts.handleFor(KIM)).toBe(contactHandle(KEY, KIM));
  });

  test("a core that cannot mint is tried a few times, then names nobody and says why, with no timer left", async () => {
    const host = setup(contactsOf());
    host.failMints();
    host.contacts.followSession("alice");
    await host.answerMints();
    const delays: number[] = [];
    while (host.timers.length > 0) {
      delays.push(...host.timers.map((timer) => timer.delay));
      await host.runTimers();
      await host.answerMints();
    }
    expect(delays).toEqual([...MINT_RETRY_MS]);
    expect([
      host.contacts.status(),
      host.contacts.handleFor(KIM),
      host.logged,
    ]).toEqual([
      "failed",
      null,
      ["The core minted no contact handles: Error: no core"],
    ]);
    // A later change asks again.
    host.change([{ account: KIM, name: "kim.01" }]);
    expect(host.mints).toHaveLength(1);
  });

  test("a failed index differs from one that names nobody", async () => {
    const host = setup([]);
    host.contacts.followSession("alice");
    await host.answerMints();
    expect(host.contacts.status()).toBe("ready");
  });

  test("a retry a new session or a removal overtook mints nothing", async () => {
    const host = setup(contactsOf());
    host.failMints(1);
    host.contacts.followSession("alice");
    await host.answerMints();
    host.contacts.followSession(null);
    expect([host.timers[0]!.cancelled, host.contacts.status()]).toEqual([
      true,
      "signed-out",
    ]);
    await host.runTimers();
    expect(host.mints).toEqual([]);

    // A retry that runs anyway, after a change it missed, is dropped as stale.
    host.failMints(1);
    host.contacts.followSession("alice");
    await host.answerMints();
    const stale = host.timers.splice(0)[0]!;
    host.change([{ account: SAM, name: "sam.01" }]);
    stale.task();
    expect(host.mints.map((mint) => mint.accounts)).toEqual([[SAM]]);
  });

  test("signed out, the host asks the core for nothing", () => {
    const host = setup(contactsOf());
    host.change([{ account: KIM, name: "kim.01" }]);
    host.newSession();
    expect([host.mints, host.contacts.status()]).toEqual([[], "signed-out"]);
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
