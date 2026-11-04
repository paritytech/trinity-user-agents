import type { FactSource } from "./ledger.js";

/**
 * The small pieces the Statements and Chat views draw with: elements, chips,
 * source tags, one-line facts and the "why" that opens to their detail.
 */

/** Where a fact came from: a ledger source, worked out here, or the product loader. */
export type ToolSource = FactSource | "derived" | "loader";

export const SOURCE_TITLES: Record<ToolSource, string> = {
  "light-client":
    "This tab's light client: its answer, or a page of what peers sent it. Not a People node's store",
  node: "People's RPC node: its answer, or a page of what it sent. From its own store, which the host trusts; other nodes may hold something else",
  core: "The core's answer to the host",
  host: "Held by this host",
  chain: "Read from chain state at a block",
  product: "Reported by the product; the host did not check it",
  inspector: "Seen by the inspector",
  derived:
    "Worked out here from what this tab saw and when: not what the network holds now",
  loader:
    "Counted by this host's product loader in the product's frame, before the product started",
};

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== undefined) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function button(className: string, text: string, title: string) {
  const node = el("button", className, text);
  node.type = "button";
  node.title = title;
  node.dataset.focusKey = `${className}:${title}`;
  return node;
}

/** A source tag: where the fact beside it came from. */
export function tag(source: ToolSource): HTMLElement {
  const node = el(
    "span",
    "si-src",
    source === "product"
      ? "reported"
      : source === "light-client"
        ? "light client"
        : source === "node"
          ? "People node"
          : source,
  );
  node.dataset.source = source;
  node.title = SOURCE_TITLES[source];
  return node;
}

export function clock(at: number): string {
  const date = new Date(at);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

export function bytes(count: number): string {
  return count < 1024 ? `${count} B` : `${(count / 1024).toFixed(1)} KiB`;
}

/** A fact row: label, value, and where the value came from. */
export function fact(
  label: string,
  value: string | Node,
  source: ToolSource,
  note?: string,
): HTMLElement[] {
  const dt = el("dt", undefined, label);
  const dd = el("dd");
  dd.append(tag(source), " ", value);
  if (note !== undefined) dd.append(more(note.trim()));
  return [dt, dd];
}

/** A table cell: `text`, and a "why" that opens to `detail`. */
export function cell(text: string, detail: string): HTMLElement {
  const node = el("span", "si-cell", text);
  node.append(more(detail));
  return node;
}

/**
 * The disclosures open under `root`, by a key that survives a redraw: the
 * nearest `data-key` above, the fact's label, and its place among the
 * disclosures there.
 */
export function openDisclosures(root: HTMLElement): Set<string> {
  return new Set(
    [...root.querySelectorAll<HTMLDetailsElement>("details[open]")].map(
      disclosureKey,
    ),
  );
}

/** Open again the disclosures {@link openDisclosures} found before a redraw. */
export function reopenDisclosures(root: HTMLElement, keys: Set<string>): void {
  if (keys.size === 0) return;
  for (const details of root.querySelectorAll<HTMLDetailsElement>("details"))
    if (keys.has(disclosureKey(details))) details.open = true;
}

function disclosureKey(details: HTMLDetailsElement): string {
  const owner = details.parentElement?.closest<HTMLElement>("[data-key]");
  const dd = details.closest("dd");
  const label =
    dd?.previousElementSibling?.tagName === "DT"
      ? dd.previousElementSibling.textContent
      : "";
  const scope = owner ?? details.ownerDocument.body;
  const peers = [...scope.querySelectorAll("details")].filter(
    (other) => other.closest("[data-key]") === details.closest("[data-key]"),
  );
  return `${owner?.dataset.key ?? ""}|${label}|${details.dataset.key ?? peers.indexOf(details)}`;
}

/** A short "why" that opens, by click or tap, to `text`. */
export function more(text: string): HTMLDetailsElement {
  const details = el("details", "si-why");
  details.append(el("summary", undefined, "why"), el("span", "si-note", text));
  return details;
}
