import { describe, expect, test } from "bun:test";
import { findWidget } from "../../../src/pocket/widget.js";

const NAME = "example.paseo";
const ROOT = JSON.stringify({
  $v: 1,
  displayName: "Example",
  description: "A synthetic product.",
  icon: { cid: "bafkicon", format: "png" },
});
const WIDGET = JSON.stringify({
  $v: 1,
  kind: "widget",
  appVersion: [0, 1, 0],
  dimensions: { height: [8] },
});
const APP = JSON.stringify({ $v: 1, kind: "app", appVersion: [0, 1, 0] });

/** Records keyed `name#key` for text and `name` for content; anything else is absent. */
function readers(
  records: Record<string, string>,
  kinds: Record<string, "car" | "site" | "other"> = {},
) {
  const read: string[] = [];
  return {
    read,
    readText: async (record: string, key: string) => {
      read.push(`${record}#${key}`);
      return records[`${record}#${key}`] ?? null;
    },
    readCid: async (record: string) => {
      read.push(record);
      return records[record] ?? null;
    },
    classify: async (cid: string) => kinds[cid] ?? "car",
  };
}

/** A product as published: a root manifest, an app, and a base name holding the app's content. */
const PUBLISHED = {
  [`${NAME}#manifest`]: ROOT,
  [`app.${NAME}#executable`]: APP,
  [`app.${NAME}`]: "bafyapp",
  [NAME]: "bafybase",
};

describe("findWidget", () => {
  test("opens the content of widget.<name> when the product publishes a widget", async () => {
    const from = readers({
      ...PUBLISHED,
      [`widget.${NAME}#executable`]: WIDGET,
      [`widget.${NAME}`]: "bafywidget",
    });
    expect(await findWidget(NAME, from)).toEqual({
      record: `widget.${NAME}`,
      cid: "bafywidget",
      kind: "car",
      fromBaseName: false,
    });
    expect(from.read.some((record) => record.startsWith("app."))).toBe(false);
  });

  // The iOS host serves a product with no widget from its base name, with the
  // card in the query, and never from `app.<name>`.
  test("opens the base name's content when the product publishes no widget", async () => {
    const from = readers(PUBLISHED);
    expect(await findWidget(NAME, from)).toEqual({
      record: NAME,
      cid: "bafybase",
      kind: "car",
      fromBaseName: true,
    });
    expect(from.read.some((record) => record.startsWith("app."))).toBe(false);
  });

  test("opens the base name's content for a product with no root manifest, without reading its subnames", async () => {
    const from = readers({ [NAME]: "bafysite" }, { bafysite: "site" });
    expect(await findWidget(NAME, from)).toEqual({
      record: NAME,
      cid: "bafysite",
      kind: "site",
      fromBaseName: true,
    });
    expect(from.read).toEqual([`${NAME}#manifest`, NAME]);
  });

  test("opens nothing when neither a widget nor the base name has content", async () => {
    await expect(
      findWidget(NAME, readers({ [`app.${NAME}`]: "bafyapp" })),
    ).rejects.toThrow("publishes no widget and has no content of its own");
    await expect(
      findWidget(
        NAME,
        readers({ [`${NAME}#manifest`]: ROOT, [`app.${NAME}`]: "bafyapp" }),
      ),
    ).rejects.toThrow("has no content of its own");
  });

  test("refuses, and says why, a manifest that is published but not valid", async () => {
    for (const [root, why] of [
      ["not json", "not JSON"],
      [JSON.stringify({ ...JSON.parse(ROOT), $v: 2 }), "version 2"],
      [JSON.stringify({ ...JSON.parse(ROOT), displayName: "" }), "displayName"],
      [JSON.stringify({ ...JSON.parse(ROOT), icon: undefined }), "icon.cid"],
    ] as const)
      await expect(
        findWidget(NAME, readers({ ...PUBLISHED, [`${NAME}#manifest`]: root })),
      ).rejects.toThrow(new RegExp(`root manifest is not usable: .*${why}`));

    for (const [manifest, why] of [
      [APP, "not a widget"],
      [JSON.stringify({ ...JSON.parse(WIDGET), $v: 2 }), "version 2"],
      [JSON.stringify({ ...JSON.parse(WIDGET), dimensions: {} }), "no height"],
      ["not json", "not JSON"],
    ] as const)
      await expect(
        findWidget(
          NAME,
          readers({
            ...PUBLISHED,
            [`widget.${NAME}#executable`]: manifest,
            [`widget.${NAME}`]: "bafywidget",
          }),
        ),
      ).rejects.toThrow(
        new RegExp(`widget\\.${NAME}'s manifest is not usable: .*${why}`),
      );
  });

  test("refuses a widget with no content, and content that is neither an archive nor a directory", async () => {
    await expect(
      findWidget(
        NAME,
        readers({ ...PUBLISHED, [`widget.${NAME}#executable`]: WIDGET }),
      ),
    ).rejects.toThrow(`widget.${NAME} has no content`);
    await expect(
      findWidget(NAME, readers(PUBLISHED, { bafybase: "other" })),
    ).rejects.toThrow("neither an archive");
  });
});
