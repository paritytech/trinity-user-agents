import { describe, expect, test } from "bun:test";
import {
  findWorker,
  parseWorkerManifest,
} from "../../../src/products/worker-manifest.js";

/** A worker manifest in the shape `dotkit` publishes to a `worker.` record. */
const PUBLISHED =
  '{"$v":1,"kind":"worker","appVersion":[0,1,0],"entrypoint":"index.js","includes":{"chat":true,"pocket":false}}';

describe("parseWorkerManifest", () => {
  test("reads a published worker manifest", () => {
    expect(parseWorkerManifest(PUBLISHED)).toEqual({
      appVersion: [0, 1, 0],
      entrypoint: "index.js",
      includes: { chat: true, pocket: false, input: false },
      pocketCards: [],
    });
  });

  test("reads the Pocket cards a worker that serves Pocket draws", () => {
    const pocket = (cards: unknown, includesPocket = true) =>
      parseWorkerManifest(
        JSON.stringify({
          ...JSON.parse(PUBLISHED),
          includes: { chat: false, pocket: includesPocket },
          pocket: { cards },
        }),
      ).pocketCards;
    const card = { id: "membership", title: "Membership", preview: "p.json" };
    expect(pocket([card, { ...card, id: "badge", faceShown: false }])).toEqual([
      "membership",
      "badge",
    ]);
    // Cards are read only behind `includes.pocket`.
    expect(pocket([card], false)).toEqual([]);
    // One bad definition drops every card, and only the cards.
    for (const bad of [
      { ...card, id: "" },
      { ...card, title: " " },
      { id: "x", title: "X" },
      { ...card, faceShown: "yes" },
    ])
      expect(pocket([card, bad])).toEqual([]);
    expect(pocket([card, card])).toEqual([]);
    expect(pocket("not a list")).toEqual([]);
  });

  test("refuses what is not a worker manifest", () => {
    const with_ = (change: Record<string, unknown>) =>
      JSON.stringify({ ...JSON.parse(PUBLISHED), ...change });
    for (const [json, reason] of [
      ["not json", "not JSON"],
      ["[]", "not an object"],
      [with_({ $v: 2 }), "version 2"],
      [with_({ kind: "app" }), "app, not a worker"],
      [with_({ entrypoint: "../index.js" }), "entry point"],
      [with_({ entrypoint: "index.html" }), "entry point"],
      [with_({ entrypoint: "/index.js" }), "entry point"],
      [with_({ includes: { chat: "yes" } }), "includes.chat"],
      [with_({ appVersion: [0, 1] }), "appVersion"],
    ] as const)
      expect(() => parseWorkerManifest(json)).toThrow(reason);
  });
});

describe("findWorker", () => {
  const readers = (records: {
    text?: string | null;
    cid?: string | null;
    kind?: "car" | "site" | "other";
  }) => ({
    readText: async () => records.text ?? null,
    readCid: async () => records.cid ?? null,
    classify: async () => records.kind ?? "car",
  });

  test("finds the manifest and the content of worker.<name>", async () => {
    expect(
      await findWorker(
        "example.paseo",
        readers({ text: PUBLISHED, cid: "bafyfixture" }),
      ),
    ).toMatchObject({
      record: "worker.example.paseo",
      cid: "bafyfixture",
      kind: "car",
      manifest: { entrypoint: "index.js" },
    });
  });

  test("says what is missing", async () => {
    await expect(
      findWorker("x.paseo", readers({ cid: "bafy" })),
    ).rejects.toThrow("no executable manifest");
    await expect(
      findWorker("x.paseo", readers({ text: PUBLISHED })),
    ).rejects.toThrow("has no content");
    await expect(
      findWorker("x.paseo", readers({ text: "{}", cid: "bafy" })),
    ).rejects.toThrow("not usable");
    await expect(
      findWorker(
        "x.paseo",
        readers({ text: PUBLISHED, cid: "bafy", kind: "other" }),
      ),
    ).rejects.toThrow("neither an archive");
  });
});
