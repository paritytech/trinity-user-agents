import {
  FieldError,
  type FieldKind,
  type FieldTheme,
} from "../../bridges/fields/protocol.js";

/**
 * PROTOTYPE. How a host draws a field value as an SVG image: text only, no
 * script, link, style sheet, image or any other reference, so the image
 * never fetches or runs anything. The size is fixed per kind, so a value's
 * length does not show in the image's size. Long values wrap into more rows
 * and then a smaller type; a value that still does not fit is refused, never
 * cut. The rules are plain enough for a native host to produce the same
 * output.
 *
 * Widths are an estimate, not a measurement: a narrow grapheme counts
 * {@link NARROW} of the type size, and a wide one (CJK, Hangul, fullwidth
 * forms, emoji) a whole em. A font much wider than that can still run past
 * the right edge.
 */

const WIDTH = 360;
const HEIGHTS: Record<FieldKind, number> = { address: 150, phone: 26 };
/** Room the address keeps under its rows for the line saying who drew it. */
const SEAL = 18;
const LARGEST = 14;
const SMALLEST = 9;
/** A generous estimate of a narrow grapheme's width, per point of type size, bold included. */
const NARROW = 0.62;
const LEADING = 1.35;

const COLORS: Record<FieldTheme, { ink: string; muted: string; bg: string }> = {
  paper: { ink: "#16171b", muted: "#6b6d75", bg: "#fbfaf6" },
  dark: { ink: "#f4f4f5", muted: "#8c8f98", bg: "#17181c" },
};

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
/** Graphemes drawn about a whole em wide. */
const WIDE =
  /\p{Extended_Pictographic}|\p{Regional_Indicator}|[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\u3000-\u303f\uff01-\uff60\uffe0-\uffe6]/u;

/** A grapheme's estimated width, in ems. */
const widthOf = (grapheme: string): number =>
  WIDE.test(grapheme) ? 1 : NARROW;

const split = (text: string): string[] =>
  [...graphemes.segment(text)].map((part) => part.segment);

/**
 * Text as SVG character data: the five XML specials escaped, and every
 * character XML 1.0 does not allow (controls, lone surrogates, U+FFFE and
 * U+FFFF) replaced with U+FFFD.
 */
export function xmlText(text: string): string {
  return text
    .replace(
      // eslint-disable-next-line no-control-regex
      /[\u0000-\u0008\u000b\u000c\u000e-\u001f\ufffe\uffff]|[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g,
      "\ufffd",
    )
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/**
 * `line` in rows no wider than `room` ems: broken at spaces, and between
 * graphemes where a word, or text without spaces, is wider than a row.
 * Every grapheme is kept; only the spaces rows break at are dropped.
 */
function wrap(line: string, room: number): string[] {
  const rows: string[] = [];
  let row: string[] = [];
  let width = 0;
  const push = () => {
    rows.push(row.join("").trimEnd());
    row = [];
    width = 0;
  };
  for (const grapheme of split(line)) {
    const space = grapheme === " ";
    if (space && row.length === 0) continue;
    const size = widthOf(grapheme);
    if (width + size > room) {
      // Break at the last space in the row, when there is one.
      const at = row.lastIndexOf(" ");
      if (at > 0 && !space) {
        const rest = row.slice(at + 1);
        row = row.slice(0, at);
        push();
        row = rest;
        width = rest.reduce((sum, part) => sum + widthOf(part), 0);
      } else push();
      if (space) continue;
    }
    row.push(grapheme);
    width += size;
  }
  if (row.length > 0) push();
  return rows;
}

/** The largest type size whose rows fit, and those rows, or null when none does. */
function layout(
  lines: readonly string[],
  height: number,
): { size: number; rows: string[] } | null {
  for (let size = LARGEST; size >= SMALLEST; size -= 0.5) {
    const rows = lines.flatMap((line) => wrap(line, WIDTH / size));
    if (rows.length * size * LEADING <= height) return { size, rows };
  }
  return null;
}

const room = (kind: FieldKind) =>
  kind === "address" ? HEIGHTS[kind] - SEAL : HEIGHTS[kind];

/** Whether `lines` of a `kind` fit its image at the smallest type. */
export const fitsField = (kind: FieldKind, lines: readonly string[]): boolean =>
  layout(lines, room(kind)) !== null;

/**
 * The SVG image of a field value's `lines`. Throws a `limit`
 * {@link FieldError} when they do not fit, rather than drop any.
 */
export function fieldSvg(
  kind: FieldKind,
  lines: readonly string[],
  theme: FieldTheme,
): string {
  const placed = layout(lines, room(kind));
  if (placed === null)
    throw new FieldError("limit", "This value is too long to draw.");
  const { size, rows } = placed;
  const height = HEIGHTS[kind];
  const color = COLORS[theme];
  const address = kind === "address";
  const step = size * LEADING;
  const text = rows
    .map(
      (row, index) =>
        `<text x="0" y="${(size + index * step).toFixed(1)}"${index === 0 && address ? ' font-weight="600"' : ""}>${xmlText(row)}</text>`,
    )
    .join("");
  const seal = address
    ? `<text x="0" y="${height - 4}" font-size="10" fill="${color.muted}">Private · drawn by your Polkadot host</text>`
    : "";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${height}" viewBox="0 0 ${WIDTH} ${height}"><rect width="100%" height="100%" fill="${color.bg}"/><g font-family="Inter, system-ui, -apple-system, sans-serif" font-size="${size}" fill="${color.ink}">${text}${seal}</g></svg>`;
}
