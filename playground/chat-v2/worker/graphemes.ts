/**
 * Split a text into user-perceived characters, for `/allowed`. The function
 * uses `Intl.Segmenter` when the runtime has it. Otherwise a small rule set
 * joins the code points of one emoji: ZWJ sequences, variation selectors,
 * skin tones, keycaps, tags and flag pairs.
 */
export function splitGraphemes(text: string): string[] {
  const compact = text.replace(/\s+/g, "");
  if (compact === "") return [];
  const Segmenter = (Intl as { Segmenter?: typeof Intl.Segmenter }).Segmenter;
  if (Segmenter) {
    return Array.from(
      new Segmenter(undefined, { granularity: "grapheme" }).segment(compact),
      (part) => part.segment,
    );
  }
  return splitFallback(compact);
}

const ZWJ = 0x200d;

function isExtender(code: number): boolean {
  return (
    code === ZWJ ||
    (code >= 0xfe00 && code <= 0xfe0f) ||
    (code >= 0x1f3fb && code <= 0x1f3ff) ||
    code === 0x20e3 ||
    (code >= 0xe0020 && code <= 0xe007f) ||
    (code >= 0x0300 && code <= 0x036f)
  );
}

function isRegionalIndicator(code: number): boolean {
  return code >= 0x1f1e6 && code <= 0x1f1ff;
}

export function splitFallback(text: string): string[] {
  const out: string[] = [];
  let joinNext = false;
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    const last = out.length - 1;
    const previous = last >= 0 ? (out[last].codePointAt(0) ?? 0) : 0;
    const pairsFlag =
      last >= 0 &&
      isRegionalIndicator(code) &&
      isRegionalIndicator(previous) &&
      Array.from(out[last]).length === 1;
    if (last >= 0 && (joinNext || isExtender(code) || pairsFlag)) {
      out[last] += char;
    } else {
      out.push(char);
    }
    joinNext = code === ZWJ;
  }
  return out;
}
