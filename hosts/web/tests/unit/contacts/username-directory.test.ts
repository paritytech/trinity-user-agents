import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import {
  type DiscoveryView,
  type ListedUser,
  SearchFailure,
  type UsernameDirectory,
  UsernameDiscovery,
  exactUsernameDirectory,
  fixtureDirectory,
  routeDirectory,
  ss58Account,
  usernamePrefix,
} from "../../../src/contacts/username-directory.js";

/** The public development account Alice, in its generic (42) and Polkadot (0) forms. */
const DEV_ALICE = {
  generic: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
  polkadot: "15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5",
  account:
    "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d" as HexString,
};

const account = (byte: string) => `0x${byte.repeat(32)}` as HexString;
const user = (username: string, byte: string): ListedUser => ({
  username,
  account: account(byte),
});

describe("usernamePrefix", () => {
  test.each([
    ["alice", "alice"],
    [" Alice ", "alice"],
    ["alice.", "alice."],
    ["alice.06", "alice.06"],
    ["a", "a"],
  ])("takes %p as %p", (typed, prefix) => {
    expect(usernamePrefix(typed)).toBe(prefix);
  });

  test.each(["", "   ", "al ice", "alice.x", ".06", "ali-ce", "a".repeat(65)])(
    "refuses %p, as the index would",
    (typed) => {
      expect(usernamePrefix(typed)).toBeNull();
    },
  );
});

describe("ss58Account", () => {
  test("reads the account of any network's form", () => {
    expect([
      ss58Account(DEV_ALICE.generic),
      ss58Account(DEV_ALICE.polkadot),
    ]).toEqual([DEV_ALICE.account, DEV_ALICE.account]);
  });

  test.each([
    ["a changed character", DEV_ALICE.generic.replace("G", "H")],
    ["a character outside base58", `${DEV_ALICE.generic.slice(0, -1)}0`],
    ["a short address", DEV_ALICE.generic.slice(0, 20)],
    ["an empty string", ""],
  ])("refuses %s", (_, address) => {
    expect(ss58Account(address)).toBeNull();
  });
});

describe("routeDirectory", () => {
  const page = (body: unknown, init: ResponseInit = {}) =>
    Response.json(body, init);

  function route(
    answer: () => Response | Promise<Response>,
    owner: (username: string) => Promise<HexString | null> = async () => null,
  ) {
    const asked: { url: string; init: RequestInit | undefined }[] = [];
    const directory = routeDirectory({
      base: "/host/",
      network: "previewnet",
      owner,
      fetch: (async (url: string, init?: RequestInit) => {
        asked.push({ url, init });
        return answer();
      }) as typeof fetch,
    });
    return { asked, directory };
  }

  const search = (directory: UsernameDirectory, cursor: string | null = null) =>
    directory.search("ali", 20, cursor, new AbortController().signal);

  test("asks this host's route for the tab's network, without cookies", async () => {
    const { asked, directory } = route(() =>
      page({
        usernames: [{ accountId: DEV_ALICE.generic, username: "alice.06" }],
        nextCursor: "next",
      }),
    );
    expect(await search(directory, "here")).toEqual({
      users: [{ account: DEV_ALICE.account, username: "alice.06" }],
      nextCursor: "next",
    });
    expect(asked.map(({ url, init }) => [url, init?.credentials])).toEqual([
      [
        "/host/__username-search?network=previewnet&prefix=ali&limit=20&cursor=here",
        "omit",
      ],
    ]);
  });

  test.each([
    ["an account that is not SS58", { accountId: "0x1234", username: "alice" }],
    [
      "a bad checksum",
      { accountId: DEV_ALICE.generic.replace("G", "H"), username: "alice" },
    ],
    [
      "a username that is not one",
      { accountId: DEV_ALICE.generic, username: "<b>" },
    ],
    ["a missing account", { username: "alice" }],
  ])("refuses a page with %s rather than show it", async (_, entry) => {
    const { directory } = route(() =>
      page({ usernames: [entry], nextCursor: null }),
    );
    await expect(search(directory)).rejects.toBeInstanceOf(SearchFailure);
  });

  test("a static server's page is not taken for an empty answer", async () => {
    const { directory } = route(
      () =>
        new Response("<!doctype html>", {
          headers: { "content-type": "text/html" },
        }),
    );
    await expect(search(directory)).rejects.toThrow(
      "Username search is not available on this server.",
    );
  });

  test("a rate limit says how long to wait", async () => {
    const { directory } = route(() =>
      page(
        { error: "Rate limit exceeded." },
        { status: 429, headers: { "retry-after": "30" } },
      ),
    );
    const failure = await search(directory).catch((error) => error);
    expect([failure.message, failure.retryAfterMs]).toEqual([
      "Too many searches. Try again in 30 s.",
      30_000,
    ]);
  });

  test("the backend's own refusal is shown as it says it", async () => {
    const { directory } = route(() =>
      page({ error: "Invalid cursor" }, { status: 400 }),
    );
    await expect(search(directory, "stale")).rejects.toThrow("Invalid cursor");
  });

  test("an unreachable server is a failure, a cancelled search is not reported as one", async () => {
    const { directory } = route(() => Promise.reject(new TypeError("offline")));
    await expect(search(directory)).rejects.toThrow(
      "Username search is unreachable.",
    );
    const cancelled = new AbortController();
    cancelled.abort();
    await expect(
      directory.search("ali", 20, null, cancelled.signal),
    ).rejects.toBeInstanceOf(TypeError);
  });
});

describe("routeDirectory.confirm", () => {
  const listed = { username: "alice.06", account: DEV_ALICE.account };
  const confirm = (owner: (username: string) => Promise<HexString | null>) =>
    routeDirectory({ base: "/", network: "paseo", owner }).confirm(listed);

  test("confirms a username that names the listed account on chain", async () => {
    const asked: string[] = [];
    const problem = await confirm(async (username) => {
      asked.push(username);
      return DEV_ALICE.account.toUpperCase().replace("0X", "0x") as HexString;
    });
    expect([problem, asked]).toEqual([null, ["alice.06"]]);
  });

  test.each([
    [
      "names nobody on chain",
      async () => null,
      "No account holds alice.06 on chain.",
    ],
    [
      "names another account on chain",
      async () => account("b0"),
      "On chain, alice.06 names another account than the index lists.",
    ],
    [
      "cannot be read",
      async () => {
        throw new Error("the chain is not connected");
      },
      "Could not check alice.06 on chain: the chain is not connected",
    ],
  ])("refuses a username that %s", async (_, owner, problem) => {
    expect(await confirm(owner)).toBe(problem);
  });
});

/** A directory whose answers the test releases one by one. */
function heldDirectory() {
  const calls: {
    prefix: string;
    cursor: string | null;
    signal: AbortSignal;
    answer(page: { users: ListedUser[]; nextCursor: string | null }): void;
    fail(error: Error): void;
  }[] = [];
  const directory: UsernameDirectory = {
    source: "a held index",
    confirm: async () => null,
    search: (prefix, _limit, cursor, signal) =>
      new Promise((resolve, reject) =>
        calls.push({ prefix, cursor, signal, answer: resolve, fail: reject }),
      ),
  };
  return { calls, directory };
}

function discovery(
  directory: UsernameDirectory,
  options: { known?: string[]; now?: () => number } = {},
) {
  const views: DiscoveryView[] = [];
  const known = new Set(options.known ?? []);
  const search = new UsernameDiscovery({
    directory,
    known: (account) => known.has(account),
    show: (view) => views.push(view),
    delayMs: 0,
    now: options.now,
  });
  return { search, views, last: () => views.at(-1) };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

describe("UsernameDiscovery", () => {
  test("nothing typed, or something that is not a username, sends nothing", async () => {
    const { calls, directory } = heldDirectory();
    const { search, views } = discovery(directory);
    search.type("");
    search.type("bob smith");
    await settle();
    expect([calls.length, views]).toEqual([
      0,
      [{ tag: "idle" }, { tag: "invalid" }],
    ]);
  });

  test("an answer to an older query is dropped once a newer one is typed", async () => {
    const { calls, directory } = heldDirectory();
    const { search, last } = discovery(directory);
    search.type("al");
    await settle();
    search.type("ali");
    await settle();
    expect(calls.map((call) => [call.prefix, call.signal.aborted])).toEqual([
      ["al", true],
      ["ali", false],
    ]);
    calls[1].answer({ users: [user("alice", "a1")], nextCursor: null });
    calls[0].answer({ users: [user("alan", "a2")], nextCursor: null });
    await settle();
    expect(last()).toEqual({
      tag: "found",
      users: [user("alice", "a1")],
      more: false,
    });
  });

  test("typing fast sends one search, for the last query", async () => {
    const { calls, directory } = heldDirectory();
    const search = new UsernameDiscovery({
      directory,
      known: () => false,
      show: () => {},
      delayMs: 20,
    });
    for (const text of ["a", "al", "ali", "alic"]) search.type(text);
    await new Promise((resolve) => setTimeout(resolve, 40));
    expect(calls.map((call) => call.prefix)).toEqual(["alic"]);
  });

  test("pages add to the list, without contacts and without anyone twice", async () => {
    const { calls, directory } = heldDirectory();
    const { search, last } = discovery(directory, { known: [account("b0")] });
    search.type("a");
    await settle();
    calls[0].answer({
      users: [user("ann", "a1"), user("anne", "b0")],
      nextCursor: "page-2",
    });
    await settle();
    expect(last()).toEqual({
      tag: "found",
      users: [user("ann", "a1")],
      more: true,
    });
    search.more();
    await settle();
    expect(calls[1].cursor).toBe("page-2");
    calls[1].answer({
      users: [user("ann", "a1"), user("anna", "a3")],
      nextCursor: null,
    });
    await settle();
    expect(last()).toEqual({
      tag: "found",
      users: [user("ann", "a1"), user("anna", "a3")],
      more: false,
    });
  });

  test("closing cancels the search in flight and drops its answer", async () => {
    const { calls, directory } = heldDirectory();
    const { search, views } = discovery(directory);
    search.type("ali");
    await settle();
    const before = views.length;
    search.close();
    calls[0].answer({ users: [user("alice", "a1")], nextCursor: null });
    search.type("bob");
    await settle();
    expect([calls[0].signal.aborted, calls.length, views.length]).toEqual([
      true,
      1,
      before,
    ]);
  });

  test("a failure keeps what was found and says why, never an empty result", async () => {
    const { calls, directory } = heldDirectory();
    const { search, last } = discovery(directory);
    search.type("a");
    await settle();
    calls[0].answer({ users: [user("ann", "a1")], nextCursor: "page-2" });
    await settle();
    search.more();
    await settle();
    calls[1].fail(new SearchFailure("The identity backend is unreachable."));
    await settle();
    expect(last()).toEqual({
      tag: "failed",
      users: [user("ann", "a1")],
      message: "The identity backend is unreachable.",
    });
  });

  test("after a rate limit nothing is sent until the wait is over", async () => {
    let now = 0;
    const { calls, directory } = heldDirectory();
    const { search, last } = discovery(directory, { now: () => now });
    search.type("a");
    await settle();
    calls[0].fail(new SearchFailure("Too many searches.", 30_000));
    await settle();
    now = 10_000;
    search.type("ab");
    await settle();
    expect([calls.length, last()]).toEqual([
      1,
      {
        tag: "failed",
        users: [],
        message: "Too many searches. Try again in 20 s.",
      },
    ]);
    now = 30_000;
    search.type("abc");
    await settle();
    expect(calls.map((call) => call.prefix)).toEqual(["a", "abc"]);
  });
});

describe("fixtureDirectory", () => {
  test("matches by prefix and pages by position", async () => {
    const directory = fixtureDirectory("test users", [
      user("carol", "c4"),
      user("bob", "b0"),
      user("bobby", "b1"),
    ]);
    const signal = new AbortController().signal;
    const first = await directory.search("bob", 1, null, signal);
    const second = await directory.search("bob", 1, first.nextCursor, signal);
    expect([first, second]).toEqual([
      { users: [user("bob", "b0")], nextCursor: "1" },
      { users: [user("bobby", "b1")], nextCursor: null },
    ]);
  });
});

describe("exactUsernameDirectory", () => {
  const signal = new AbortController().signal;

  test("finds a person only by their whole username, read on chain", async () => {
    const asked: string[] = [];
    const directory = exactUsernameDirectory(async (username) => {
      asked.push(username);
      return username === "alice.06" ? account("a1") : null;
    });
    expect([
      await directory.search("alice.06", 20, null, signal),
      await directory.search("alice", 20, null, signal),
      await directory.search("alice.06", 20, "more", signal),
      asked,
    ]).toEqual([
      { users: [user("alice.06", "a1")], nextCursor: null },
      { users: [], nextCursor: null },
      { users: [], nextCursor: null },
      ["alice.06", "alice"],
    ]);
  });

  test("a failed read is a failure the picker shows, never an empty result", async () => {
    const directory = exactUsernameDirectory(async () => {
      throw new Error("the chain is not connected");
    });
    await expect(directory.search("alice", 20, null, signal)).rejects.toThrow(
      "Could not read alice on chain: the chain is not connected",
    );
  });
});
