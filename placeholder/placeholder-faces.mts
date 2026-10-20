// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * The loader face for the host's pinned Humanity card: what the Pocket shows
 * before the worker has drawn the real one (trinity-user-agents #1380).
 *
 * It is `humanityFace`'s not-enrolled card with nobody on it: the same
 * material (BASIC white plastic, 1px light edge, hairlines), the same images
 * at the same places (the Polkadot mark top-left, the QR glyph top-right, the
 * serif wordmark ghosted bottom-left), and ghost bars where the name and the
 * status word go. Every measurement is the face module's, so the swap to the
 * real face moves nothing.
 *
 *   npx tsx scripts/placeholder-faces.mts <outDir>
 */
import { mkdirSync, writeFileSync } from "node:fs";

import {
  archive,
  background,
  border,
  box,
  fillHeight,
  fillWidth,
  height,
  image,
  opacity,
  paddingEach,
  rounded,
  width,
  type ColorToken,
  type ContentAlignment,
  type Modifier,
  type RendererNode,
} from "../packages/shared/src/renderer/index.ts";

// `humanity-face.ts`
const CARD_RADIUS = 24;
const EDGE = 16;
const TOP = 12;
const MARK_WIDTH = 44;
const MARK_HEIGHT = 45;
const MARK_GAP = 8;
const NAME_LINE = 22;
const WORDS_TOP = 3;
const QR_SIZE = 24;
const QR_INSET = 22;
const WORDMARK_WIDTH = 221;
const WORDMARK_HEIGHT = 45;
const WORDMARK_ALPHA = 0.14;
const GROUND = 11;
const LIGHT = 244;

/** BASIC: ramp [214, 180] read halfway, ink #8C8F98 over it, 1px FgPrimary edge. */
const GREY = 197;
const INK_ALPHA = (197 - 143) / (197 - GROUND);

/** `TitleMediumRegular` and `BodySmallRegular` line boxes, as the host lays them out. */
const NAME_BAR = { width: 132, height: 14 };
const STATUS_BAR = { width: 88, height: 10 };

const alphaOf = (alpha: number) => Math.round(Math.min(1, Math.max(0, alpha)) * 255);
const layer = (child: RendererNode, align: ContentAlignment, modifiers: Modifier[] = []) =>
  box([child], { modifiers: [fillWidth(), fillHeight(), ...modifiers], contentAlignment: align });

function ramp(): RendererNode {
  return box([], {
    modifiers: [fillWidth(), fillHeight(), opacity(alphaOf((GREY - GROUND) / (LIGHT - GROUND))), background("FgPrimary")],
  });
}

function hairlines(): RendererNode[] {
  const line = (color: ColorToken, alpha: number) =>
    box([], { modifiers: [fillWidth(), height(1), opacity(alphaOf(alpha)), background(color)] });
  return [layer(line("FgPrimary", 0.22), "TopStart"), layer(line("BgSurfaceMain", 0.15), "BottomStart")];
}

function edge(): RendererNode {
  return box([], { modifiers: [fillWidth(), fillHeight(), border(1, "FgPrimary", rounded(CARD_RADIUS))] });
}

function mark(): RendererNode {
  return image(archive("pocket/card-mark.webp"), { fit: "Contain", modifiers: [width(MARK_WIDTH), height(MARK_HEIGHT)] });
}

function qrGlyph(): RendererNode {
  return image(archive("pocket/qr-code.png"), { fit: "Contain", modifiers: [width(QR_SIZE), height(QR_SIZE)] });
}

function wordmark(): RendererNode {
  const img = image(archive("pocket/wordmark-dark.png"), {
    fit: "Contain",
    modifiers: [width(WORDMARK_WIDTH), height(WORDMARK_HEIGHT), opacity(alphaOf(WORDMARK_ALPHA))],
  });
  return box([img], { modifiers: [paddingEach({ top: 0, end: 0, bottom: 40, start: 0 })], contentAlignment: "TopStart" });
}

/** A ghost bar in the ink, where a line of text will be: the not-enrolled card's grey ink at a third. */
function bar({ width: w, height: h }: { width: number; height: number }, alpha: number): RendererNode {
  return box([], {
    modifiers: [width(w), height(h), opacity(alphaOf(INK_ALPHA * alpha)), background("BgSurfaceMain", rounded(h / 2))],
  });
}

/** `identity()` with the words replaced by bars, each sitting on its text's baseline-ish middle. */
function identity(bars: boolean): RendererNode {
  if (!bars) return box([mark()], { contentAlignment: "TopStart" });
  const status = box([bar(STATUS_BAR, 0.3)], {
    modifiers: [paddingEach({ top: NAME_LINE + 5, end: 0, bottom: 0, start: 0 })],
    contentAlignment: "TopStart",
  });
  const words = box([bar(NAME_BAR, 0.45), status], {
    modifiers: [paddingEach({ top: WORDS_TOP + 4, end: 0, bottom: 0, start: MARK_WIDTH + MARK_GAP })],
    contentAlignment: "TopStart",
  });
  return box([mark(), words], { contentAlignment: "TopStart" });
}

function loaderFace(bars: boolean): RendererNode {
  return box(
    [
      ramp(),
      ...hairlines(),
      layer(wordmark(), "BottomStart"),
      layer(identity(bars), "TopStart", [paddingEach({ top: TOP, end: EDGE, bottom: 0, start: EDGE })]),
      layer(qrGlyph(), "TopEnd", [paddingEach({ top: QR_INSET, end: QR_INSET, bottom: 0, start: 0 })]),
      edge(),
    ],
    { modifiers: [fillWidth(), fillHeight(), background("BgSurfaceMain", rounded(CARD_RADIUS))], contentAlignment: "TopStart" },
  );
}

const FACES: Record<string, RendererNode> = {
  loader: loaderFace(true),
  "loader-bare": loaderFace(false),
};

const outDir = process.argv[2] ?? "./placeholder-faces/";
mkdirSync(outDir, { recursive: true });
for (const [name, tree] of Object.entries(FACES)) {
  writeFileSync(`${outDir}/${name}.json`, `${JSON.stringify(tree, null, 2)}\n`);
  console.log(`wrote ${name}.json`);
}
