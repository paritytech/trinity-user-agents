import { describe, expect, test } from "bun:test";
import { FieldError } from "../../../bridges/fields/protocol.js";
import { fieldSvg, fitsField, xmlText } from "../../../src/fields/svg.js";

/** Every element and attribute name an SVG uses. */
const vocabulary = (svg: string) => ({
  elements: [...new Set([...svg.matchAll(/<([a-zA-Z]+)/g)].map((m) => m[1]))],
  attributes: [
    ...new Set([...svg.matchAll(/\s([a-zA-Z:-]+)="/g)].map((m) => m[1])),
  ].sort(),
});

const size = (svg: string) =>
  /<svg[^>]*width="(\d+)" height="(\d+)"/.exec(svg)!.slice(1).join("x");
const fontSize = (svg: string) =>
  Number(/<g [^>]*font-size="([\d.]+)"/.exec(svg)![1]);
/** The value's rows as drawn, without the line saying who drew it. */
const rowsOf = (svg: string) =>
  [
    ...svg.matchAll(
      /<text x="0" y="[\d.]+"(?: font-weight="600")?>([^<]*)<\/text>/g,
    ),
  ].map((match) => match[1]!);

/** A row's estimated width in ems, by the generator's own rule. */
const wide =
  /\p{Extended_Pictographic}|\p{Regional_Indicator}|[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\u3000-\u303f\uff01-\uff60\uffe0-\uffe6]/u;
const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const ems = (row: string) =>
  [...segmenter.segment(row)].reduce(
    (sum, part) => sum + (wide.test(part.segment) ? 1 : 0.62),
    0,
  );
/** Whether every row of `svg` fits the 360-wide image at its type size. */
const fits = (svg: string) =>
  rowsOf(svg).every((row) => ems(row) * fontSize(svg) <= 360);

const limitOf = (run: () => unknown) => {
  try {
    run();
    return null;
  } catch (error) {
    return error instanceof FieldError ? error.code : String(error);
  }
};

describe("the field SVG", () => {
  test("escapes every value and uses only text: no script, link, style or image", () => {
    const hostile = `</text><script>alert(1)</script><image href="https://x.test/a.png"/>&\u0001`;
    const svg = fieldSvg("address", [hostile], "paper");
    expect({
      escaped: xmlText(hostile),
      vocabulary: vocabulary(svg),
    }).toEqual({
      escaped:
        "&lt;/text&gt;&lt;script&gt;alert(1)&lt;/script&gt;&lt;image href=&quot;https://x.test/a.png&quot;/&gt;&amp;\ufffd",
      vocabulary: {
        elements: ["svg", "rect", "g", "text"],
        attributes: [
          "fill",
          "font-family",
          "font-size",
          "font-weight",
          "height",
          "viewBox",
          "width",
          "x",
          "xmlns",
          "y",
        ],
      },
    });
  });

  test("keeps one size per kind, and wraps and shrinks long values instead of growing", () => {
    const short = fieldSvg("address", ["A"], "paper");
    const long = fieldSvg(
      "address",
      ["x".repeat(60), "b", "c", "d", "e", "f"],
      "paper",
    );
    const sixLines = fieldSvg(
      "address",
      Array.from(
        { length: 6 },
        (_, index) => `Line ${index} of a long address`,
      ),
      "dark",
    );
    expect({
      sizes: [short, long, sixLines].map(size),
      phone: size(fieldSvg("phone", ["+1 555 010 0123"], "paper")),
      longRows: rowsOf(long).length,
      longSmaller: fontSize(long) < fontSize(short),
      longText: rowsOf(long).join(""),
      sixRows: rowsOf(sixLines).length,
      fit: [short, long, sixLines].every(fits),
    }).toEqual({
      sizes: ["360x150", "360x150", "360x150"],
      phone: "360x26",
      longRows: 7,
      longSmaller: true,
      longText: "x".repeat(60) + "bcdef",
      sixRows: 6,
      fit: true,
    });
  });

  test("counts wide scripts and emoji as wide, wraps text without spaces, and keeps every grapheme", () => {
    // Synthetic values, one per script, none a real place.
    const japanese =
      "東京都架空区試験町一丁目二番三号サンプルマンション四〇五号室";
    const korean = "서울특별시 가상구 시험로 123 예시아파트 4동 506호";
    const fullwidth = "ＡＢＣ１２３　ＤＥＦ４５６　ＧＨＩ７８９　ＪＫＬ";
    const family = "👨‍👩‍👧‍👦";
    const emoji = `${family} 🇧🇷 ${"🏠".repeat(30)}`;
    const drawn = [japanese, korean, fullwidth, emoji].map((line) =>
      fieldSvg("address", [line], "paper"),
    );
    const japaneseRows = rowsOf(drawn[0]!);
    expect({
      fit: drawn.every(fits),
      japaneseWrapped: japaneseRows.length > 1,
      japaneseKept: japaneseRows.join(""),
      koreanKept: rowsOf(drawn[1]!).join(" "),
      // The family is one grapheme and never split across rows.
      familyWhole: rowsOf(drawn[3]!).some((row) => row.includes(family)),
      emojiKept: rowsOf(drawn[3]!).join(" ").replace(/ /g, ""),
    }).toEqual({
      fit: true,
      japaneseWrapped: true,
      japaneseKept: japanese,
      koreanKept: korean,
      familyWhole: true,
      emojiKept: emoji.replace(/ /g, ""),
    });
  });

  test("refuses a value that does not fit at the smallest type instead of cutting it", () => {
    // Beyond what a provider may save (160 characters, 6 lines), so only a
    // phone can reach the refusal through the bridge.
    const tooWide = "電".repeat(500);
    expect({
      addressAtLimit: fitsField("address", ["電".repeat(160)]),
      addressWide: fitsField("address", [tooWide]),
      phoneLong: fitsField("phone", ["9".repeat(160)]),
      phoneShort: fitsField("phone", ["+1 555 010 0123"]),
      latinMax: fitsField("address", [
        "x".repeat(60),
        "x".repeat(20),
        "x".repeat(20),
        "x".repeat(20),
        "x".repeat(20),
        "x".repeat(20),
      ]),
      thrown: limitOf(() => fieldSvg("address", [tooWide], "paper")),
    }).toEqual({
      addressAtLimit: true,
      addressWide: false,
      phoneLong: false,
      phoneShort: true,
      latinMax: true,
      thrown: "limit",
    });
  });
});
