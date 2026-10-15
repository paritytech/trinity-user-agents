import {
  POCKET_TITLE,
  pocketCards,
  type DigitalDollarCard,
  type PocketCard,
  type ProductCard,
} from "./pocket.js";

/**
 * The Pocket list as the native app lays it out: a large title over cards that
 * overlap, so each card behind the last shows a strip of itself. Only the
 * layout and the faces are drawn here. A card takes no press yet, because the
 * native card opens a details page that needs the product or the balance
 * behind it.
 */

const SVG = "http://www.w3.org/2000/svg";

/** `BundledPaymentAssetLogos.Square`: the mark, in its 100 x 110 viewport. */
const MARK_PATH =
  "M43.9111 26.8311C44.542 26.5039 45.1293 27.3068 44.666 27.8457C44.6465 27.8684 44.6269 27.8913 44.6074 27.9141C34.9099 39.0281 29.8753 54.5843 32.6836 69.0117C35.6296 86.6419 48.8094 101.35 64.9482 106.978C65.5761 107.196 65.5399 108.114 64.8867 108.238C61.5562 108.874 58.1181 109.207 54.6025 109.207C39.5716 109.207 25.9603 103.132 16.0869 93.3047C13.6556 90.8848 12.193 87.6609 11.9111 84.2422C11.78 82.6512 11.7095 81.0745 11.6953 79.5254C11.6947 79.5017 11.6929 79.4768 11.6924 79.4531C11.1518 56.9035 25.3647 36.4507 43.9111 26.8311ZM41.7637 79.7275C41.4843 79.024 42.4717 78.5596 42.9258 79.165C49.9839 88.5851 61.0062 94.6474 73.3936 94.6475C82.6465 94.6471 91.137 91.2629 97.7832 85.625C98.3795 85.1191 99.3605 85.8903 98.9033 86.5244C93.3577 94.2078 85.862 100.39 77.1426 104.348C73.7739 105.877 69.9198 106.044 66.4883 104.662C55.1664 100.102 46.1391 90.7465 41.7637 79.7275ZM54.6025 0C72.8399 0.000180739 88.9874 8.94353 98.9033 22.6816C99.2217 23.1234 98.842 23.6299 98.3809 23.7266C89.7343 16.323 78.2239 12.0381 64.2998 12.9785C31.0559 15.2243 5.2568 47.0948 7.78125 79.7549C7.7953 79.9365 7.81028 80.1185 7.82617 80.2998C7.88713 80.9971 6.966 81.3118 6.63086 80.6973C2.40436 72.9448 0.000187232 64.0558 0 54.6045C0 24.4483 24.4464 0.000479843 54.6025 0Z";

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** The logo the digital dollar card opens with, 36 high like the native one. */
function logo(): SVGSVGElement {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 100 110");
  svg.setAttribute("class", "pocket-logo");
  svg.setAttribute("aria-hidden", "true");
  const defs = document.createElementNS(SVG, "defs");
  const gradient = document.createElementNS(SVG, "linearGradient");
  gradient.setAttribute("id", "pocket-logo-gradient");
  gradient.setAttribute("gradientUnits", "userSpaceOnUse");
  gradient.setAttribute("x1", "26.6498");
  gradient.setAttribute("y1", "115.326");
  gradient.setAttribute("x2", "99.0221");
  gradient.setAttribute("y2", "13.8747");
  for (const [offset, color] of [
    [0, "#070707"],
    [0.254808, "#969696"],
    [0.586538, "#4b4b4b"],
    [1, "#c5c5c5"],
  ] as const) {
    const stop = document.createElementNS(SVG, "stop");
    stop.setAttribute("offset", String(offset));
    stop.setAttribute("stop-color", color);
    gradient.append(stop);
  }
  defs.append(gradient);
  const path = document.createElementNS(SVG, "path");
  path.setAttribute("d", MARK_PATH);
  path.setAttribute("fill", "url(#pocket-logo-gradient)");
  svg.append(defs, path);
  return svg;
}

function digitalDollar(card: DigitalDollarCard): HTMLElement {
  const root = el("article", "pocket-card pocket-dollar");
  root.dataset.card = card.id;
  const artwork = el("div", "pocket-dollar-art");
  const highlight = el("div", "pocket-dollar-highlight");
  const grain = el("div", "pocket-grain pocket-dollar-grain");
  const content = el("div", "pocket-dollar-content");
  const amount = el("span", "pocket-amount", "Unavailable");
  amount.dataset.state = card.amount.state;
  amount.title = card.amount.reason;
  content.append(logo(), amount);
  root.append(artwork, highlight, grain, content);
  return root;
}

function productCard(card: ProductCard): HTMLElement {
  const root = el("article", "pocket-card pocket-product");
  root.dataset.card = card.id;
  root.append(el("div", "pocket-grain"), el("div", "pocket-face"));
  const face = root.lastElementChild!;
  face.append(
    el("span", "pocket-face-title", card.title),
    el("span", "pocket-face-subtitle", card.subtitle),
  );
  return root;
}

function drawCard(card: PocketCard): HTMLElement {
  return card.kind === "digital-dollar"
    ? digitalDollar(card)
    : productCard(card);
}

/** The Pocket list, ready to put in the screen. */
export function drawPocket(): HTMLElement[] {
  const bar = el("header", "pocket-bar");
  bar.append(el("h2", "pocket-title", POCKET_TITLE));
  const list = el("div", "pocket-list");
  for (const card of pocketCards()) {
    const slot = el("div", "pocket-slot");
    slot.append(drawCard(card));
    list.append(slot);
  }
  return [bar, list];
}
