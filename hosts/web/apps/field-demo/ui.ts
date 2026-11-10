/** Presentation helpers shared by the two field demo products. No bridge or data access. */

/** Make an element with an optional class and text. */
export function node<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  if (className !== "") made.className = className;
  if (text !== undefined) made.textContent = text;
  return made;
}

/** A stable small number for a string, to pick a color from the palette. */
export function hash(text: string): number {
  let value = 0;
  for (const char of text) value = (value * 31 + char.charCodeAt(0)) >>> 0;
  return value;
}

/**
 * Avatar colors of the Polkadot app design system (Berlin Night
 * `avatar.bg.*` / `avatar.fg.*`), in the order the tokens list them.
 */
const PALETTE = [
  "amethyst",
  "opal",
  "turquoise",
  "onyx",
  "pearl",
  "emerald",
  "topaz",
  "ruby",
  "sapphire",
  "garnet",
] as const;

/** Give an element the palette color picked by `seed`. */
export function paint(target: HTMLElement, seed: string): void {
  target.dataset.tone = PALETTE[hash(seed) % PALETTE.length];
}

/** Status messages at the top of the product. */
export function createToast(box: HTMLElement) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return (text: string, tone: "info" | "success" | "error" = "info"): void => {
    box.textContent = text;
    box.dataset.kind = tone;
    box.dataset.show = "";
    clearTimeout(timer);
    timer = setTimeout(() => delete box.dataset.show, 3_200);
  };
}

/** An inline 20×20 icon from the small set the two products use. */
export function icon(name: keyof typeof ICONS): string {
  return `<svg viewBox="0 0 20 20" aria-hidden="true">${ICONS[name]}</svg>`;
}

/** Draw every `data-icon` placeholder in the static markup. */
export function fillIcons(root: ParentNode = document): void {
  for (const slot of root.querySelectorAll<HTMLElement>("[data-icon]")) {
    const name = slot.dataset.icon;
    if (name !== undefined && name in ICONS)
      slot.innerHTML = icon(name as keyof typeof ICONS);
  }
}

const ICONS = {
  person:
    '<path fill="currentColor" d="M10 10a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm0 1.75c-3.3 0-6 1.8-6 4v.75c0 .41.34.75.75.75h10.5c.41 0 .75-.34.75-.75v-.75c0-2.2-2.7-4-6-4Z"/>',
  lock: '<path fill="currentColor" d="M10 2a4 4 0 0 0-4 4v2h-.5A1.5 1.5 0 0 0 4 9.5v7A1.5 1.5 0 0 0 5.5 18h9a1.5 1.5 0 0 0 1.5-1.5v-7A1.5 1.5 0 0 0 14.5 8H14V6a4 4 0 0 0-4-4Zm2.5 6h-5V6a2.5 2.5 0 0 1 5 0v2Z"/>',
  box: '<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M3 6.5 10 3l7 3.5v7L10 17l-7-3.5v-7Zm0 0L10 10m0 0 7-3.5M10 10v7"/>',
  truck:
    '<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M2.5 5.5h9v8h-9zM11.5 8.5h3.2l2.8 2.8v2.2h-6"/><circle cx="6" cy="14.5" r="1.6" fill="currentColor"/><circle cx="14.5" cy="14.5" r="1.6" fill="currentColor"/>',
  check:
    '<path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="m5 10.5 3.2 3L15 6.5"/>',
  search:
    '<path fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" d="M9 15a6 6 0 1 0 0-12 6 6 0 0 0 0 12Zm4.5-1.5L17 17"/>',
  printer:
    '<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M6 7V3h8v4M6 14H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-2M6 11.5h8V17H6z"/>',
  tag: '<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M3 3h6.5L17 10.5 10.5 17 3 9.5V3Z"/><circle cx="6.5" cy="6.5" r="1.3" fill="currentColor"/>',
  back: '<path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="m12 4-6 6 6 6"/>',
  chevron:
    '<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="m8 5 5 5-5 5"/>',
} as const;
