import { describe, expect, test } from "bun:test";
import type { AuthState } from "@parity/truapi-host";
import {
  accountStatusRows,
  productAllowanceRows,
  type ChainState,
} from "./account-status.js";
import type {
  ConsumerRecord,
  PeopleChainReading,
  RingMembership,
} from "./people-chain.js";

const CONTEXT = { networkSuffix: "paseo", walletName: "Dev" };
const KEY = `0x${"ab".repeat(32)}` as const;
const NO_RINGS: RingMembership[] = [
  { collection: "LitePeople", lookup: { tag: "NoKey" } },
  { collection: "People", lookup: { tag: "NoKey" } },
];

function connected(
  overrides: Partial<Extract<AuthState, { tag: "Connected" }>["value"]> = {},
): AuthState {
  return { tag: "Connected", value: { publicKey: KEY, ...overrides } };
}

const value = (state: AuthState, label: string) =>
  accountStatusRows(state, CONTEXT).find((row) => row.label === label);

describe("account status", () => {
  test("shows only the network and session while signed out", () => {
    expect(
      accountStatusRows({ tag: "Disconnected" }, CONTEXT).map(
        (row) => `${row.label}: ${row.value}`,
      ),
    ).toEqual(["Network: paseo", "Session: Signed out"]);
  });

  test("names a failed sign-in by its reason", () => {
    const failed: AuthState = {
      tag: "LoginFailed",
      value: { kind: "Other", reason: "no network" } as never,
    };
    expect(value(failed, "Session")?.value).toBe("Failed: no network");
  });

  // The session of an imported wallet carries no username, so its absence says
  // nothing; only the People chain read states standing.
  test("shows no session username unless the session reports one", () => {
    expect(value(connected(), "Session username")).toBeUndefined();
    expect(
      value(
        connected({ fullUsername: "alice", liteUsername: "alice.07" }),
        "Session username",
      ),
    ).toMatchObject({ value: "alice", detail: "full" });
    expect(
      value(connected({ liteUsername: "alice.07" }), "Session username"),
    ).toMatchObject({ value: "alice.07", detail: "lite" });
  });

  test("does not state standing before the chain was read", () => {
    const rows = (chain: ChainState) =>
      accountStatusRows(connected(), CONTEXT, chain).find(
        (row) => row.label === "People chain",
      );
    expect(rows({ state: "idle" })).toMatchObject({ value: "Not checked" });
    expect(rows({ state: "loading" })).toMatchObject({ value: "Checking…" });
    expect(rows({ state: "error", message: "no answer" })).toMatchObject({
      value: "Unavailable",
      detail: "no answer",
    });
  });

  const reading = (readings: PeopleChainReading["readings"]): ChainState => ({
    state: "done",
    checkedAt: new Date(0),
    reading: { blockHash: `0x${"cd".repeat(32)}`, readings },
  });
  const person: ConsumerRecord = {
    fullUsername: "alice",
    liteUsername: "alice.07",
    credibility: {
      tag: "Person",
      alias: `0x${"09".repeat(32)}`,
      lastUpdate: 1_700_000_000n,
      demoted: true,
    },
    slots: [
      { tag: "Occupied", accountId: `0x${"07".repeat(32)}`, since: 5n },
      { tag: "Free" },
    ],
  };

  test("states standing per queried account, with the username", () => {
    const rows = accountStatusRows(
      connected(),
      CONTEXT,
      reading([
        {
          role: "identity",
          accountId: KEY,
          consumer: person,
          litePerson: false,
          rings: NO_RINGS,
        },
        {
          role: "root",
          accountId: KEY,
          consumer: null,
          litePerson: false,
          rings: NO_RINGS,
        },
      ]),
    );
    const byLabel = Object.fromEntries(rows.map((row) => [row.label, row]));
    expect(byLabel["On chain: identity account"]).toMatchObject({
      value: "Person (demoted) · alice",
      detail: "lite alice.07",
    });
    expect(byLabel["On chain: root key"]).toMatchObject({ value: "Not found" });
    expect(byLabel["Consumer slots"]).toMatchObject({
      value: "1 of 2 occupied",
      detail: "identity account",
    });
  });

  // A record absent under both queried accounts still does not prove the
  // person is unregistered: the tooltip says where else it may be.
  test("calls a missing record not found, never unregistered", () => {
    const rows = accountStatusRows(
      connected(),
      CONTEXT,
      reading([
        {
          role: "root",
          accountId: KEY,
          consumer: null,
          litePerson: false,
          rings: NO_RINGS,
        },
      ]),
    );
    const row = rows.find((r) => r.label === "On chain: root key");
    expect(row?.value).toBe("Not found");
    expect(row?.title).toContain("different account");
    expect(rows.find((r) => r.label === "Consumer slots")?.value).toBe(
      "No record",
    );
  });

  test("tells a Lite entry without a username record from an unreadable one", () => {
    const rows = accountStatusRows(
      connected(),
      CONTEXT,
      reading([
        {
          role: "root",
          accountId: KEY,
          consumer: null,
          litePerson: true,
          rings: NO_RINGS,
        },
        {
          role: "identity",
          accountId: KEY,
          consumer: null,
          litePerson: false,
          rings: NO_RINGS,
          problem: "bad layout",
        },
      ]),
    );
    expect(rows.find((r) => r.label === "On chain: root key")).toMatchObject({
      value: "Lite person",
    });
    expect(
      rows.find((r) => r.label === "On chain: identity account"),
    ).toMatchObject({ value: "Unreadable", state: "warning" });
  });

  // The account rows never state a product allowance: it is a separate question
  // with its own group.
  test("keeps product allowance out of the account rows", () => {
    const rows = accountStatusRows(connected(), CONTEXT, reading([]));
    expect(rows.some((row) => /allowance|allocation/i.test(row.label))).toBe(
      false,
    );
  });

  test("abbreviates keys and keeps the full value in the tooltip", () => {
    const row = value(connected({ identityAccountId: KEY }), "Root key");
    expect(row?.title).toBe(KEY);
    expect(row?.value).toContain("…");
    expect(
      value(connected({ identityAccountId: KEY }), "Identity account")?.title,
    ).toContain(KEY);
  });

  test("shows the wallet name only when one is signed in", () => {
    expect(value(connected(), "Wallet")?.value).toBe("Dev");
    expect(
      accountStatusRows(connected(), { ...CONTEXT, walletName: null }).some(
        (row) => row.label === "Wallet",
      ),
    ).toBe(false);
  });
});

describe("product allowance", () => {
  const by = (rows: ReturnType<typeof productAllowanceRows>, label: string) =>
    rows.find((row) => row.label === label);

  test("names no product before one is open", () => {
    expect(productAllowanceRows({ state: "none-open" })).toEqual([
      { label: "Product", value: "None open", state: "unknown" },
    ]);
  });

  test("asks to sign in and shows loading without stating a record", () => {
    expect(
      by(
        productAllowanceRows({ state: "signed-out", productId: "a.paseo" }),
        "Allocation recorded",
      )?.value,
    ).toBe("Sign in first");
    expect(
      by(
        productAllowanceRows({ state: "loading", productId: "a.paseo" }),
        "Allocation recorded",
      )?.value,
    ).toBe("Checking…");
  });

  // "No" is about this wallet's ledger only, and must not read as the chain
  // holding nothing.
  test("reports what the ledger records, and never the chain", () => {
    const rows = (record: "recorded" | "none" | "unreadable") =>
      productAllowanceRows({ state: "done", productId: "a.paseo", record });
    expect(by(rows("recorded"), "Allocation recorded")?.value).toBe("Yes");
    expect(by(rows("none"), "Allocation recorded")).toMatchObject({
      value: "No",
    });
    expect(by(rows("none"), "Allocation recorded")?.title).toContain(
      "says nothing about the chain",
    );
    expect(by(rows("unreadable"), "Allocation recorded")).toMatchObject({
      value: "Unreadable",
      state: "warning",
    });
  });

  // Until a Refresh has read the chains there is nothing on chain to state, and
  // the ledger must not stand in for it.
  test("does not state the chain before it was read, in every recorded state", () => {
    for (const record of ["recorded", "none", "unreadable"] as const)
      expect(
        by(
          productAllowanceRows({ state: "done", productId: "a.paseo", record }),
          "On chain",
        ),
      ).toMatchObject({ value: "Not read", state: "unknown" });
  });

  test("shows a read made for another product as not read", () => {
    const rows = productAllowanceRows(
      { state: "done", productId: "a.paseo", record: "none" },
      { state: "unsupported", productId: "b.paseo" },
    );
    expect(by(rows, "On chain")).toMatchObject({ value: "Not read" });
  });

  test("shows the resource rows for the product they were read for", () => {
    const rows = productAllowanceRows(
      { state: "done", productId: "a.paseo", record: "none" },
      { state: "unsupported", productId: "a.paseo" },
    );
    expect(by(rows, "On chain")).toMatchObject({
      value: "Needs a newer core",
    });
  });
});

describe("ring membership rows", () => {
  const RING_KEY = `0x${"07".repeat(32)}` as const;
  const rowsFor = (rings: RingMembership[]) =>
    accountStatusRows(connected(), CONTEXT, {
      state: "done",
      checkedAt: new Date(0),
      reading: {
        blockHash: `0x${"cd".repeat(32)}`,
        readings: [
          {
            role: "identity",
            accountId: KEY,
            consumer: null,
            litePerson: true,
            rings,
          },
        ],
      },
    }).filter((row) => row.label.startsWith("Ring"));

  test("reports the position the chain stores for a named key", () => {
    expect(
      rowsFor([
        {
          collection: "LitePeople",
          lookup: {
            tag: "Listed",
            key: RING_KEY,
            position: {
              tag: "Included",
              ringIndex: 3,
              ringPage: 0,
              ringPosition: 12,
            },
          },
        },
        { collection: "People", lookup: { tag: "NoKey" } },
      ]),
    ).toMatchObject([
      {
        label: "Ring: lite",
        value: "Included · ring 3",
        detail: "identity account, position 12",
      },
    ]);
  });

  test("flags a suspended key and an unreadable record", () => {
    const rows = rowsFor([
      {
        collection: "LitePeople",
        lookup: {
          tag: "Listed",
          key: RING_KEY,
          position: { tag: "Suspended" },
        },
      },
      { collection: "People", lookup: { tag: "Unreadable", reason: "moved" } },
    ]);
    expect(rows).toMatchObject([
      { value: "Suspended", state: "warning" },
      { label: "Ring: full", value: "Unreadable", state: "warning" },
    ]);
  });

  // A key with no Members entry is a chain fact. No key at all is only that
  // nothing named one, and must never be worded as not being a member.
  test("says no key on record, not non-membership, when none is named", () => {
    const [row] = rowsFor(NO_RINGS);
    expect(row).toMatchObject({ value: "No ring key on record" });
    expect(row.value).not.toMatch(/not a member|outside/i);
    expect(row.title).toContain("not a finding");
    expect(
      rowsFor([
        {
          collection: "LitePeople",
          lookup: { tag: "NotListed", key: RING_KEY },
        },
        { collection: "People", lookup: { tag: "NoKey" } },
      ]),
    ).toMatchObject([{ value: "No Members entry" }]);
  });
});
