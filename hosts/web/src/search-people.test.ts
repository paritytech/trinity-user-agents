import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "./chat-provider.js";
import type { PickedPerson } from "./contacts.js";
import { SearchPeople } from "./search-people.js";
import {
  fixtureDirectory,
  type UsernameDirectory,
} from "./username-directory.js";

const account = (byte: string) => `0x${byte.repeat(32)}` as HexString;
const KIM = account("4b");
const SAM = account("5a");
const ALINE = account("a1");
const ALVA = account("a2");

const settle = () => new Promise((resolve) => setTimeout(resolve, 5));

function setup(
  options: {
    available?: boolean;
    contacts?: ChatContact[];
    blocked?: HexString[];
    directory?: UsernameDirectory;
  } = {},
) {
  const chosen: PickedPerson[] = [];
  let available = options.available ?? true;
  let blocked = options.blocked ?? [];
  const people = new SearchPeople({
    available: () => available,
    contacts: () =>
      options.contacts ?? [
        { account: KIM, name: "kim.01" },
        { account: SAM, name: "sam.01" },
      ],
    blocked: () => blocked,
    directory: () =>
      options.directory ??
      fixtureDirectory("test", [
        { account: ALINE, username: "aline.01" },
        { account: ALVA, username: "alva.02" },
        { account: KIM, username: "kim.01" },
      ]),
    changed: () => {},
    chose: (person) => chosen.push(person),
    delayMs: 0,
  });
  const labels = () =>
    people.rows().map((row) => [row.label, row.note] as const);
  return {
    people,
    chosen,
    labels,
    setAvailable: (value: boolean) => (available = value),
    setBlocked: (value: HexString[]) => (blocked = value),
  };
}

describe("the Docklet's people", () => {
  test("offers matching contacts, then people found by username, and picks no one while typing", async () => {
    const { people, chosen, labels } = setup();
    people.type("");
    expect(people.shown()).toBe(false);
    people.type("k");
    await settle();
    expect(labels()).toEqual([["kim.01", null]]);
    people.type("al");
    expect(people.status()).toBe("Searching usernames…");
    await settle();
    // Kim is a contact, so the index does not list Kim again.
    expect(labels()).toEqual([
      ["aline.01", "Not in your contacts"],
      ["alva.02", "Not in your contacts"],
    ]);
    expect(people.status()).toBeNull();
    expect(chosen).toEqual([]);
  });

  test("a contact is chosen as one; a person found by username is checked first", async () => {
    let release = (_problem: string | null) => {};
    const directory: UsernameDirectory = {
      ...fixtureDirectory("test", [{ account: ALINE, username: "aline.01" }]),
      confirm: () => new Promise((resolve) => (release = resolve)),
    };
    const { people, chosen } = setup({ directory });
    people.type("kim");
    await settle();
    people.rows()[0]!.choose();
    expect(chosen).toEqual([{ account: KIM, name: "kim.01", found: false }]);

    people.type("aline");
    await settle();
    people.rows()[0]!.choose();
    expect(people.status()).toBe("Checking aline.01 on chain…");
    // While one is checked, no other row answers.
    people.rows()[0]!.choose();
    release("aline.01 names someone else on chain.");
    await settle();
    expect(chosen).toHaveLength(1);
    expect(people.status()).toBe("aline.01 names someone else on chain.");

    people.rows()[0]!.choose();
    release(null);
    await settle();
    expect(chosen[1]).toEqual({
      account: ALINE,
      name: "aline.01",
      found: true,
    });
  });

  test("typing again lets a person still being checked go", async () => {
    let release = (_problem: string | null) => {};
    const directory: UsernameDirectory = {
      ...fixtureDirectory("test", [{ account: ALINE, username: "aline.01" }]),
      confirm: () => new Promise((resolve) => (release = resolve)),
    };
    const { people, chosen } = setup({ directory });
    people.type("aline");
    await settle();
    people.rows()[0]!.choose();
    people.type("kim");
    await settle();
    expect(people.status()).toBeNull();
    people.rows()[0]!.choose();
    release(null);
    await settle();
    expect(chosen).toEqual([{ account: KIM, name: "kim.01", found: false }]);
  });

  test("a check that ends after a reset, such as a new session, answers nothing", async () => {
    let release = (_problem: string | null) => {};
    const directory: UsernameDirectory = {
      ...fixtureDirectory("test", [{ account: ALINE, username: "aline.01" }]),
      confirm: () => new Promise((resolve) => (release = resolve)),
    };
    const { people, chosen } = setup({ directory });
    people.type("aline");
    await settle();
    people.rows()[0]!.choose();
    people.reset();
    release(null);
    await settle();
    expect(chosen).toEqual([]);
    expect(people.shown()).toBe(false);
  });

  test("offers no one who is blocked, and no one at all where Chat cannot show a person", async () => {
    const { people, labels, setAvailable } = setup({ blocked: [KIM, ALVA] });
    people.type("k");
    await settle();
    expect(labels()).toEqual([]);
    people.type("al");
    await settle();
    expect(labels()).toEqual([["aline.01", "Not in your contacts"]]);

    setAvailable(false);
    expect(people.shown()).toBe(false);
    people.type("aline");
    await settle();
    expect(labels()).toEqual([]);
    expect(people.status()).toBeNull();
  });

  test("text that cannot be a username, such as a URL, shows no people", async () => {
    const { people } = setup();
    people.type("https://example.com/app");
    await settle();
    expect(people.shown()).toBe(false);
  });
});
