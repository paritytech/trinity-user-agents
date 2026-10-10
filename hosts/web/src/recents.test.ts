import { describe, expect, test } from "bun:test";
import {
  RECENT_LIMIT,
  RecentProducts,
  isRecordable,
  matchRecents,
} from "./recents.js";
import { MemoryStorage } from "./test-support.js";

const entry = (address: string, productId = address, entered = false) => ({
  address,
  productId,
  entered,
});

describe("recent products", () => {
  test("keeps newest first, once per address and id, and bounded", () => {
    const recents = new RecentProducts(new MemoryStorage());
    recents.record("w", entry("a.paseo"), "paseo", 1);
    recents.record("w", entry("b.paseo"), "paseo", 2);
    recents.record("w", entry("a.paseo"), "paseo", 3);
    expect(recents.list("w").map((item) => item.address)).toEqual([
      "a.paseo",
      "b.paseo",
    ]);
    for (let n = 0; n < RECENT_LIMIT + 5; n += 1)
      recents.record("w", entry(`p${n}.paseo`), "paseo", 10 + n);
    expect(recents.list("w")).toHaveLength(RECENT_LIMIT);
  });

  // The same address under a different entered id is a different way to open it.
  test("remembers the product id with the address", () => {
    const recents = new RecentProducts(new MemoryStorage());
    recents.record(
      "w",
      entry("http://localhost:3000/", "chat.paseo", true),
      "paseo",
      1,
    );
    recents.record(
      "w",
      entry("http://localhost:3000/", "localhost:3000"),
      "paseo",
      2,
    );
    expect(recents.list("w")).toEqual([
      {
        address: "http://localhost:3000/",
        productId: "localhost:3000",
        entered: false,
        at: 2,
      },
      {
        address: "http://localhost:3000/",
        productId: "chat.paseo",
        entered: true,
        at: 1,
      },
    ]);
  });

  test("keeps each wallet's history apart, and forgets it with the wallet", () => {
    const recents = new RecentProducts(new MemoryStorage());
    recents.record("alice", entry("a.paseo"), "paseo", 1);
    recents.record("bob", entry("b.paseo"), "paseo", 1);
    recents.forget("alice");
    expect(recents.list("alice")).toEqual([]);
    expect(recents.list("bob")).toHaveLength(1);
  });

  // The same wallet on two networks keeps two histories, and forgetting the
  // wallet drops both.
  test("keeps each network's history apart, and forgets every network", () => {
    const recents = new RecentProducts(new MemoryStorage());
    recents.record("w", entry("a.paseo"), "paseo", 1);
    recents.record("w", entry("b.previewnet"), "previewnet", 2);
    expect(recents.list("w", "paseo").map((item) => item.address)).toEqual([
      "a.paseo",
    ]);
    expect(recents.list("w", "previewnet").map((item) => item.address)).toEqual(
      ["b.previewnet"],
    );
    recents.forget("w");
    expect(recents.list("w", "paseo")).toEqual([]);
    expect(recents.list("w", "previewnet")).toEqual([]);
  });

  test("reads damaged storage as empty", () => {
    const storage = new MemoryStorage();
    storage.setItem("truapi-web-signing-host:recents:v1:w", "{not json");
    expect(new RecentProducts(storage).list("w")).toEqual([]);
    storage.setItem(
      "truapi-web-signing-host:recents:v1:w",
      JSON.stringify([1, { address: "x" }]),
    );
    expect(new RecentProducts(storage).list("w")).toEqual([]);
  });
});

// An address that carries a secret still opens, but is never written down.
describe("what is not remembered", () => {
  test("skips credentials and secret-looking parameters", () => {
    for (const address of [
      "https://user:pw@example.test/",
      "https://example.test/cb?access_token=abc",
      "https://example.test/#token=abc",
      "https://example.test/?a=1&mnemonic=word",
      "app.paseo/x?code=1",
      "https://example.test/#/a;session=1",
    ])
      expect(isRecordable(address)).toBe(false);
    const recents = new RecentProducts(new MemoryStorage());
    expect(recents.record("w", entry("https://example.test/?token=1"))).toBe(
      false,
    );
    expect(recents.list("w")).toEqual([]);
  });

  test("keeps ordinary addresses, including a path that merely mentions a word", () => {
    for (const address of [
      "app.paseo",
      "app.paseo/keys/list?page=2",
      "http://localhost:3000/#/home",
      "https://example.test/?q=tokenizer",
    ])
      expect(isRecordable(address)).toBe(true);
  });
});

describe("matching typed text", () => {
  test("filters by address or id, and offers everything for empty text", () => {
    const all = [
      { ...entry("chat.paseo"), at: 2 },
      { ...entry("http://localhost:3000/", "dev.paseo", true), at: 1 },
    ];
    expect(matchRecents(all, "")).toEqual(all);
    expect(matchRecents(all, "CHA")).toEqual([all[0]]);
    expect(matchRecents(all, "dev")).toEqual([all[1]]);
    expect(matchRecents(all, "zzz")).toEqual([]);
  });
});
