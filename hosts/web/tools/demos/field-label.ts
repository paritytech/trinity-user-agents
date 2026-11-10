import type { ShippingLabel } from "../../src/fields/host.js";

/**
 * DEV ONLY. Prints a shipping label from a host document. Product frames run
 * sandboxed without `allow-modals`, so a product cannot call `print()`; the
 * host composes the label from its own records (the order, the listing it
 * saw, the opened address) and prints that. Nothing here reaches a product.
 */

const LABEL_CSS = `
@font-face { font-family: "Inter"; font-weight: 100 900; src: url("${new URL("../../src/assets/fonts/inter-variable-latin.woff2", import.meta.url).href}") format("woff2"); }
@font-face { font-family: "Manrope"; font-weight: 200 800; src: url("${new URL("../../src/assets/fonts/manrope-variable-latin.woff2", import.meta.url).href}") format("woff2"); }
@page { size: 4in 6in; margin: 0.2in; }
* { box-sizing: border-box; }
html, body { margin: 0; color: #111; background: #fff; font-family: "Inter", system-ui, sans-serif; }
.label { border: 2px solid #111; border-radius: 10px; padding: 16px 18px; display: flex; flex-direction: column; gap: 14px; min-height: 5.5in; }
.top { display: flex; justify-content: space-between; align-items: baseline; border-bottom: 2px solid #111; padding-bottom: 10px; }
.brand { font-family: "Manrope", sans-serif; font-weight: 800; font-size: 18px; letter-spacing: -0.01em; }
.code { font: 600 13px ui-monospace, Menlo, monospace; letter-spacing: 0.06em; }
.k { font-size: 10px; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; color: #555; margin: 0 0 6px; }
address { font-style: normal; font-size: 19px; line-height: 1.35; }
address span { display: block; }
address span:first-child { font-weight: 700; }
.row { display: flex; justify-content: space-between; gap: 12px; font-size: 13px; border-top: 1px dashed #999; padding-top: 10px; }
.foot { margin-top: auto; font-size: 9px; color: #666; line-height: 1.4; }
`;

function el<K extends keyof HTMLElementTagNameMap>(
  doc: Document,
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = doc.createElement(tag);
  if (className !== "") node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** The order code a person reads: the start of the order id, upper case. */
export const orderCode = (order: string): string =>
  `#${order.replace(/-/g, "").slice(0, 8).toUpperCase()}`;

/** Fill `doc` with the label for `label`. Text only, through `textContent`. */
export function drawLabel(doc: Document, label: ShippingLabel): void {
  doc.title = `Shipping label ${orderCode(label.order)}`;
  const style = doc.createElement("style");
  style.textContent = LABEL_CSS;
  doc.head.append(style);
  const top = el(doc, "div", "top");
  top.append(
    el(doc, "span", "brand", "Shipping label"),
    el(doc, "span", "code", orderCode(label.order)),
  );
  const to = el(doc, "section", "");
  const address = el(doc, "address", "");
  for (const line of label.address) address.append(el(doc, "span", "", line));
  to.append(el(doc, "p", "k", "Ship to"), address);
  const contents = el(doc, "div", "row");
  contents.append(
    el(doc, "span", "", `1 × ${label.item}`),
    el(
      doc,
      "span",
      "",
      new Date(label.at || Date.now()).toLocaleDateString(undefined, {
        day: "numeric",
        month: "short",
        year: "numeric",
      }),
    ),
  );
  const foot = el(
    doc,
    "p",
    "foot",
    `Printed by your Polkadot host for ${label.product}. The app never received this address.`,
  );
  const sheet = el(doc, "main", "label");
  sheet.append(top, to, contents, foot);
  doc.body.replaceChildren(sheet);
}

/** How long a label document may stay after `print()` returns without `afterprint`. */
const LABEL_LIFETIME_MS = 60_000;

/**
 * Print `label` from a hidden host frame. Settles when the frame is removed:
 * after printing, when `signal` aborts, or after {@link LABEL_LIFETIME_MS}.
 * An abort before `print()` prints nothing; a print already handed to the
 * browser cannot be recalled.
 */
export function printLabel(
  label: ShippingLabel,
  signal: AbortSignal,
): Promise<void> {
  if (signal.aborted) return Promise.reject(new Error("Session ended."));
  const frame = document.createElement("iframe");
  frame.setAttribute("aria-hidden", "true");
  frame.tabIndex = -1;
  frame.dataset.fieldLabel = "";
  frame.style.cssText =
    "position:fixed;right:0;bottom:0;width:0;height:0;border:0;opacity:0;pointer-events:none";
  document.body.append(frame);
  return new Promise<void>((resolve, reject) => {
    let removed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const remove = (error?: Error) => {
      if (removed) return;
      removed = true;
      clearTimeout(timer);
      signal.removeEventListener("abort", aborted);
      frame.remove();
      if (error === undefined) resolve();
      else reject(error);
    };
    const aborted = () => remove(new Error("Session ended."));
    signal.addEventListener("abort", aborted, { once: true });
    timer = setTimeout(() => remove(), LABEL_LIFETIME_MS);
    const win = frame.contentWindow;
    if (win === null)
      return remove(new Error("The host could not prepare the label."));
    drawLabel(win.document, label);
    void win.document.fonts.ready.then(() => {
      if (removed) return;
      win.addEventListener("afterprint", () => setTimeout(() => remove(), 0), {
        once: true,
      });
      win.focus();
      win.print();
    });
  });
}
