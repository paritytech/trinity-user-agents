import "./field-view.css";

/**
 * DEV ONLY. Runs in a host-origin frame that a product embeds where a private
 * field belongs, such as the ship-to block of a packing slip. It redeems the
 * single-use token in its fragment with the host page (`window.top`), which
 * answers only a frame whose parent is the product the token was issued to.
 * The product chooses where the frame goes and its size, never its content:
 * the browser's origin boundary keeps this document's text and pixels out of
 * the product's reach.
 */

const root = document.getElementById("view")!;
const theme =
  new URLSearchParams(location.search).get("theme") === "dark"
    ? "dark"
    : "paper";
document.documentElement.dataset.theme = theme;
const token = location.hash.slice(1);
history.replaceState(null, "", location.pathname + location.search);

const LOCK =
  '<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 1.5a3.5 3.5 0 0 0-3.5 3.5v2H4a1.5 1.5 0 0 0-1.5 1.5v5A1.5 1.5 0 0 0 4 15h8a1.5 1.5 0 0 0 1.5-1.5v-5A1.5 1.5 0 0 0 12 7h-.5V5A3.5 3.5 0 0 0 8 1.5Zm2 5.5H6V5a2 2 0 1 1 4 0v2Z"/></svg>';

function show(lines: readonly string[] | null, note: string): void {
  root.removeAttribute("aria-busy");
  if (lines === null) {
    const text = document.createElement("p");
    text.className = "note";
    text.textContent = note;
    root.replaceChildren(text);
    return;
  }
  const address = document.createElement("address");
  address.className = "lines";
  for (const line of lines) {
    const span = document.createElement("span");
    span.textContent = line;
    address.append(span);
  }
  const seal = document.createElement("p");
  seal.className = "seal";
  seal.innerHTML = LOCK;
  seal.append(note);
  root.replaceChildren(address, seal);
  fit();
}

/**
 * Shrink the text until the whole address fits the frame the product chose.
 * The frame's size is the product's; the address never changes it, so its
 * length does not leak through layout, and no line is cut.
 */
function fit(): void {
  for (let size = 15; size >= 9; size -= 0.5) {
    root.style.setProperty("--size", `${size}px`);
    if (root.scrollHeight <= root.clientHeight) return;
  }
}
addEventListener("resize", fit);

const top = window.top;
if (top === null || top === window || !/^[0-9a-f]{32}$/.test(token)) {
  show(null, "Address unavailable here.");
} else {
  let answered = false;
  const channel = new MessageChannel();
  channel.port1.onmessage = (event: MessageEvent<unknown>) => {
    answered = true;
    const data = event.data as {
      lines?: unknown;
      clear?: unknown;
    } | null;
    if (
      Array.isArray(data?.lines) &&
      data.lines.every((line) => typeof line === "string")
    )
      show(data.lines as string[], "Private · shown by your Polkadot host");
    else if (data?.clear === true)
      show(null, "Access ended. Reopen the order.");
    else show(null, "Address unavailable. Reopen the order.");
  };
  // Tell the host when this view goes away, so it can drop the view's port.
  addEventListener("pagehide", () => {
    channel.port1.postMessage({ release: true });
    channel.port1.close();
  });
  top.postMessage({ type: "web-field-view", token }, location.origin, [
    channel.port2,
  ]);
  setTimeout(() => {
    if (!answered) show(null, "Address unavailable here.");
  }, 5_000);
}
