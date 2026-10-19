// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * Candidate default faces for the host's pinned Humanity card: what the
 * Pocket shows before the worker has drawn the real one (trinity-user-agents
 * #1380). Same anatomy and measurements as `humanity-face.ts` (24dp corners,
 * 16/12 inset, 44x45 mark slot, name line at 22, wordmark 40 up), but nothing
 * the host would have to fetch: no images, so no worker archive and no
 * network. The mark and the serif wordmark are stood in for by a ghost disc
 * and a ghosted sans "Polkadot".
 *
 *   npx tsx scripts/placeholder-faces.mts <outDir>
 */
import { mkdirSync, writeFileSync } from "node:fs";

import {
  background,
  border,
  box,
  effect,
  fillHeight,
  fillWidth,
  height,
  opacity,
  paddingEach,
  rounded,
  circle,
  text,
  width,
  type ColorToken,
  type ContentAlignment,
  type Modifier,
  type RendererNode,
  type TypographyStyle,
} from "../packages/shared/src/renderer/index.ts";

const CARD_RADIUS = 24;
const EDGE = 16;
const TOP = 12;
const MARK_WIDTH = 44;
const MARK_HEIGHT = 45;
const MARK_GAP = 8;
const NAME_LINE = 22;
const WORDS_TOP = 3;
const WORDMARK_BOTTOM = 40;
const GROUND = 11;
const LIGHT = 244;

interface Ink {
  color: ColorToken;
  alpha?: number;
}

interface Material {
  /** The one grey the face is painted in, 0-255 on the dark theme's ramp. */
  grey: number;
  ink: Ink;
  ink2: Ink;
  edge?: { ink: Ink; width: number };
  foil?: number;
}

const alphaOf = (alpha: number) => Math.round(Math.min(1, Math.max(0, alpha)) * 255);
const darkInk = (target: number, over: number): Ink => ({ color: "BgSurfaceMain", alpha: (over - target) / (over - GROUND) });
const inkOpacity = (ink: Ink, extra = 1): Modifier[] => {
  const alpha = (ink.alpha ?? 1) * extra;
  return alpha >= 1 ? [] : [opacity(alphaOf(alpha))];
};
const inkText = (content: string, style: TypographyStyle, ink: Ink, extra = 1, modifiers: Modifier[] = []) =>
  text(content, { style, color: ink.color, modifiers: [...inkOpacity(ink, extra), ...modifiers] });
const layer = (child: RendererNode, align: ContentAlignment, modifiers: Modifier[] = []) =>
  box([child], { modifiers: [fillWidth(), fillHeight(), ...modifiers], contentAlignment: align });

/** `humanity-face.ts`'s BASIC: the not-enrolled card a new user gets first. */
const WHITE_PLASTIC: Material = {
  grey: 197,
  ink: darkInk(16, 197),
  ink2: darkInk(111, 197),
  edge: { ink: { color: "FgPrimary" }, width: 1 },
};

/** SILVER_FOIL at its mid-height grey, with the rainbow foil. */
const SILVER: Material = {
  grey: 226,
  ink: { color: "BgSurfaceMain" },
  ink2: darkInk(114, 226),
  foil: 0.3,
};

/** The black tier: light edge, holo. */
const BLACK: Material = {
  grey: 18,
  ink: { color: "FgPrimary" },
  ink2: { color: "FgPrimary", alpha: 0.62 },
  edge: { ink: { color: "FgPrimary", alpha: 0.65 }, width: 1 },
  foil: 0.35,
};

function ramp({ grey }: Material): RendererNode {
  return box([], {
    modifiers: [fillWidth(), fillHeight(), opacity(alphaOf((grey - GROUND) / (LIGHT - GROUND))), background("FgPrimary")],
  });
}

function foil(alpha: number): RendererNode {
  return box([effect("Rainbow", [box([], { modifiers: [fillWidth(), fillHeight()] })])], {
    modifiers: [fillWidth(), fillHeight(), opacity(alphaOf(alpha))],
  });
}

function hairlines(): RendererNode[] {
  const line = (color: ColorToken, alpha: number) =>
    box([], { modifiers: [fillWidth(), height(1), opacity(alphaOf(alpha)), background(color)] });
  return [layer(line("FgPrimary", 0.22), "TopStart"), layer(line("BgSurfaceMain", 0.15), "BottomStart")];
}

function edge({ ink, width: size }: NonNullable<Material["edge"]>): RendererNode {
  return box([], {
    modifiers: [fillWidth(), fillHeight(), ...inkOpacity(ink), border(size, ink.color, rounded(CARD_RADIUS))],
  });
}

/** Where the Polkadot mark goes: a disc in the ink at a tenth, the mark's size. */
function markSlot(material: Material): RendererNode {
  return box([], {
    modifiers: [width(MARK_WIDTH), height(MARK_HEIGHT), ...inkOpacity(material.ink, 0.1), background(material.ink.color, circle())],
  });
}

/** A ghost bar where a line of text will be. */
function bar(material: Material, w: number, h: number, alpha: number): RendererNode {
  return box([], {
    modifiers: [width(w), height(h), ...inkOpacity(material.ink, alpha), background(material.ink.color, rounded(h / 2))],
  });
}

/** The words in the name and status slots, or ghost bars in their place. */
function identity(material: Material, words: { title: string; status: string } | null): RendererNode {
  const status = box(
    [words ? inkText(words.status.toUpperCase(), "BodySmallRegular", material.ink2) : bar(material, 96, 10, 0.12)],
    { modifiers: [paddingEach({ top: NAME_LINE + (words ? 0 : 4), end: 0, bottom: 0, start: 0 })], contentAlignment: "TopStart" },
  );
  const name = words ? inkText(words.title, "TitleMediumRegular", material.ink) : bar(material, 140, 14, 0.18);
  const wordsBox = box([name, status], {
    modifiers: [paddingEach({ top: WORDS_TOP + (words ? 0 : 3), end: 0, bottom: 0, start: MARK_WIDTH + MARK_GAP })],
    contentAlignment: "TopStart",
  });
  return box([markSlot(material), wordsBox], { contentAlignment: "TopStart" });
}

/** The ghosted wordmark, in the host's own type since the serif is an image. */
function wordmark(material: Material): RendererNode {
  return box([inkText("Polkadot", "HeadlineLarge", { color: material.ink.color }, 0.14)], {
    modifiers: [paddingEach({ top: 0, end: 0, bottom: WORDMARK_BOTTOM, start: EDGE })],
    contentAlignment: "TopStart",
  });
}

function face(material: Material, words: { title: string; status: string } | null): RendererNode {
  return box(
    [
      ramp(material),
      ...(material.foil === undefined ? [] : [foil(material.foil)]),
      ...hairlines(),
      layer(wordmark(material), "BottomStart"),
      layer(identity(material, words), "TopStart", [paddingEach({ top: TOP, end: EDGE, bottom: 0, start: EDGE })]),
      ...(material.edge === undefined ? [] : [edge(material.edge)]),
    ],
    { modifiers: [fillWidth(), fillHeight(), background("BgSurfaceMain", rounded(CARD_RADIUS))], contentAlignment: "TopStart" },
  );
}

const WORDS = { title: "Humanity", status: "Proof of personhood" };

const FACES: Record<string, RendererNode> = {
  "placeholder-white": face(WHITE_PLASTIC, WORDS),
  "placeholder-silver": face(SILVER, WORDS),
  "placeholder-black": face(BLACK, WORDS),
  "placeholder-skeleton": face(WHITE_PLASTIC, null),
};

const outDir = process.argv[2] ?? "./placeholder-faces/";
mkdirSync(outDir, { recursive: true });
for (const [name, tree] of Object.entries(FACES)) {
  writeFileSync(`${outDir}/${name}.json`, `${JSON.stringify(tree, null, 2)}\n`);
  console.log(`wrote ${name}.json`);
}
