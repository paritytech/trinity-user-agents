import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { base64ToBytes, bytesToBase64, bytesToHex, dataUrlToHex, hexLength, hexToUtf8 } from "../../worker/bytes";
import {
  customBody,
  FIXED_PARTICIPANTS,
  INLINE_IMAGE_LIMIT,
  INLINE_MEDIA_LIMIT,
  STREAM_WORDS,
  toolbar,
  type Assets,
} from "../../worker/fixtures";
import { appDeeplink, roomLink } from "../../worker/links";

const ASSETS = join(import.meta.dir, "../../assets");

function asset(name: string) {
  return bytesToHex(new Uint8Array(readFileSync(join(ASSETS, name))));
}

function assets(): Assets {
  return {
    avatar: asset("avatar.png"),
    avatarAlt: asset("avatar-alt.png"),
    badge: asset("badge.png"),
    icons: [1, 2, 3, 4, 5, 6].map((i) => asset(`icon-${i}.png`)),
    samplePng: asset("sample.png"),
    samplePdf: asset("sample.pdf"),
    sampleMp4: asset("sample.mp4"),
  };
}

describe("assets", () => {
  test("images and icons are under 64 KiB, the PDF and the video under 1 MiB", () => {
    for (const name of readdirSync(ASSETS)) {
      const size = statSync(join(ASSETS, name)).size;
      const limit = name.endsWith(".png") ? INLINE_IMAGE_LIMIT : INLINE_MEDIA_LIMIT;
      expect({ name, fits: size > 0 && size < limit }).toEqual({ name, fits: true });
    }
  });

  test("every asset of the spec exists", () => {
    const names = new Set(readdirSync(ASSETS));
    for (const name of ["avatar.png", "avatar-alt.png", "badge.png", "sample.png", "sample.pdf", "sample.mp4"]) {
      expect(names.has(name)).toBe(true);
    }
  });

  test("a data URL decodes to the file bytes", () => {
    const bytes = readFileSync(join(ASSETS, "sample.pdf"));
    expect(dataUrlToHex(`data:application/pdf;base64,${bytes.toString("base64")}`)).toBe(asset("sample.pdf"));
  });
});

describe("bytes", () => {
  test("base64 encodes every tail length and round-trips", () => {
    for (const text of ["", "f", "fo", "foo", "foob", "fooba", "foobar"]) {
      const bytes = new TextEncoder().encode(text);
      expect(bytesToBase64(bytes)).toBe(Buffer.from(text).toString("base64"));
      expect(base64ToBytes(bytesToBase64(bytes))).toEqual(bytes);
    }
  });
});

describe("fixtures", () => {
  test("p1 to p4 are a person, a lite person, a contact and a product", () => {
    expect(FIXED_PARTICIPANTS.map((p) => [p.participantId, p.participant.tag])).toEqual([
      ["p1", "Person"],
      ["p2", "LitePerson"],
      ["p3", "Contact"],
      ["p4", "Product"],
    ]);
    const lite = FIXED_PARTICIPANTS[1].participant;
    if (lite.tag === "LitePerson") expect(hexLength(lite.value.account)).toBe(32);
  });

  test("the toolbar opens the App first and calls back for the rest", () => {
    const actions = toolbar(3, assets(), "sample.dot");
    expect(actions.map((a) => a.actionId)).toEqual(["tb-1", "tb-2", "tb-3"]);
    expect(actions[0].handler).toEqual({ tag: "Deeplink", value: { url: "polkadot://sample.dot/?from=deeplink" } });
    expect(actions[1].handler).toEqual({ tag: "Callback" });
    expect(toolbar(0, assets())).toEqual([]);
  });

  test("/stream sends forty words", () => {
    expect(STREAM_WORDS.length).toBe(40);
  });

  test("the custom card carries its counter", () => {
    const body = customBody(7);
    expect(body.tag).toBe("Custom");
    if (body.tag === "Custom") expect(hexToUtf8(body.value.payload)).toBe("0007");
  });

  test("room links follow the RFC", () => {
    expect(roomLink("source", "sample.dot")).toBe("polkadot://sample.dot/-/chat/source");
    expect(appDeeplink("sample.dot")).toBe("polkadot://sample.dot/?from=deeplink");
  });
});
