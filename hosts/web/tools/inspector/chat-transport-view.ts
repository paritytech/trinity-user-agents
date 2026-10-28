import { bytesToHex } from "@parity/truapi/scale";
import type {
  CapturedFrame,
  CapturedStatement,
  PayloadCapture,
} from "./capture.js";
import { statementOf, type ChatEvent } from "./diagnostics.js";
import { decodePlaintextLayer, decodeStatementLayer } from "./inspect.js";

/**
 * The Transport part of a chat event's detail, in three layers over what the
 * session captured for it:
 *
 * - Raw: the JSON-RPC frames and the encoded statement, exactly as they
 *   travelled. Public: the store and anyone reading it see the same.
 * - Decrypted: the statement's data in the clear, exactly as decryption
 *   returned it or as it was before sealing. Sensitive.
 * - Decoded: both read as structured data. Derived, never what travelled.
 */

export type Layer = "raw" | "decrypted" | "decoded";

const LAYERS: { layer: Layer; label: string }[] = [
  { layer: "raw", label: "Raw" },
  { layer: "decrypted", label: "Decrypted" },
  { layer: "decoded", label: "Decoded" },
];

/** What a copy button copies, kept off the DOM so large payloads stay out of attributes. */
const copies = new WeakMap<HTMLElement, () => string>();

/** The text a transport copy button copies, if `button` is one. */
export const transportCopyText = (button: HTMLElement): string | undefined =>
  copies.get(button)?.();

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== undefined) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function copyButton(label: string, text: () => string): HTMLButtonElement {
  const node = element("button", "ins-method", label);
  node.type = "button";
  node.dataset.copyLayer = "";
  copies.set(node, text);
  return node;
}

const size = (bytes: number): string =>
  bytes < 1024
    ? `${bytes} B`
    : bytes < 1024 * 1024
      ? `${(bytes / 1024).toFixed(1)} KB`
      : `${(bytes / 1024 / 1024).toFixed(2)} MB`;

/** One captured value: a heading, what it is, and the value itself. */
function block(
  title: string,
  note: string,
  value: string,
  copyLabel: string,
): HTMLElement {
  const section = element("section", "cdv-block");
  const head = element("div", "cdv-block-head");
  head.append(
    element("span", "cdv-block-title", title),
    element("span", "cdv-block-note", note),
    copyButton(copyLabel, () => value),
  );
  section.append(head, element("pre", "td-detail-pre cdv-bytes", value));
  return section;
}

function frameBlock(frame: CapturedFrame, hash: string | null): HTMLElement {
  const what =
    frame.method ?? (frame.subscription !== null ? "notification" : "answer");
  const position =
    hash === null || frame.statements.length < 2
      ? ""
      : ` · carries ${frame.statements.length} statements, this is number ${frame.statements.indexOf(hash) + 1}`;
  const cut =
    frame.text.length < frame.length
      ? ` · cut to ${frame.text.length} of ${frame.length} characters`
      : "";
  return block(
    `JSON-RPC #${frame.seq} ${frame.direction === "in" ? "◀ received" : "▶ sent"} · ${what} · connection ${frame.connection}`,
    `exact text ${frame.direction === "in" ? "as received from" : "as sent to"} the connection · ${frame.length} characters${position}${cut}`,
    frame.text,
    "copy text",
  );
}

function badge(kind: "public" | "sensitive" | "derived", text: string) {
  return element("p", `cdv-layer-note cdv-layer-${kind}`, text);
}

function gap(text: string): HTMLElement {
  return element("p", "cdv-gap", text);
}

function rawLayer(
  statement: CapturedStatement,
  frames: CapturedFrame[],
): HTMLElement[] {
  const nodes: HTMLElement[] = [
    badge(
      "public",
      "Public: these bytes travel as they are, and the store and anyone reading it see them.",
    ),
  ];
  if (frames.length === 0)
    nodes.push(
      gap(
        "No JSON-RPC frame holds this statement: none was captured for it, or it was evicted.",
      ),
    );
  for (const frame of frames) nodes.push(frameBlock(frame, statement.hash));
  const cut =
    statement.raw.length < statement.length
      ? ` · cut to ${statement.raw.length} of ${statement.length} bytes`
      : "";
  nodes.push(
    block(
      `Encoded signed statement · ${statement.length} bytes`,
      `${statement.direction === "in" ? "exact bytes the engine received, from the frame's hex" : "exact bytes the engine submitted"} · blake2b-256 ${statement.hash}${cut}`,
      bytesToHex(statement.raw),
      "copy hex",
    ),
  );
  return nodes;
}

function decryptedLayer(statement: CapturedStatement): HTMLElement[] {
  const nodes: HTMLElement[] = [
    badge(
      "sensitive",
      "Sensitive: decrypted content, which can hold message text and push tokens. Kept in this tab only.",
    ),
  ];
  if (statement.plaintext === null) {
    nodes.push(
      gap(
        `No decrypted bytes: ${statement.plaintextGap ?? "the engine did not decrypt this statement"}.`,
      ),
    );
    return nodes;
  }
  nodes.push(
    block(
      `Plaintext · ${statement.plaintext.length} bytes`,
      statement.direction === "in"
        ? "exact bytes decryption returned"
        : "exact bytes the engine sealed",
      bytesToHex(statement.plaintext),
      "copy hex",
    ),
  );
  return nodes;
}

/** The Decoded layer as one value: the two parts kept apart. */
export function decodedRecord(statement: CapturedStatement) {
  const read = {
    statement: decodeStatementLayer(statement.raw, statement.stream),
    plaintext:
      statement.plaintext === null
        ? null
        : decodePlaintextLayer(statement.plaintext, statement.stream),
  };
  return {
    protocol: {
      statement: read.statement.protocol,
      plaintext: read.plaintext?.protocol ?? null,
    },
    annotations: {
      statement: read.statement.annotations,
      plaintext: read.plaintext?.annotations ?? null,
    },
  };
}

function decodedLayer(statement: CapturedStatement): HTMLElement[] {
  const record = decodedRecord(statement);
  const protocol = JSON.stringify(record.protocol, null, 2);
  const annotations = JSON.stringify(record.annotations, null, 2);
  const both = element("div", "cdv-block-head");
  both.append(
    element(
      "span",
      "cdv-block-note",
      "Derived from the Raw and Decrypted bytes. Nothing travels as JSON.",
    ),
    copyButton("copy both", () => JSON.stringify(record, null, 2)),
  );
  return [
    badge(
      "derived",
      "Protocol fields are the values read from the bytes, in wire order. The binary has no names: keys are the schema's field names, and enum indexes, option tags, counts and length prefixes are kept as read. Inspector annotations are what this host adds, keyed by the protocol path they describe.",
    ),
    ...(statement.plaintext === null
      ? [gap("Only the statement is read: there are no decrypted bytes.")]
      : []),
    both,
    block(
      "Protocol fields",
      `${size(protocol.length)} of JSON`,
      protocol,
      "copy JSON",
    ),
    block(
      "Inspector annotations",
      "names, times, hashes, sizes, offsets, notes, and where reading stopped",
      annotations,
      "copy JSON",
    ),
  ];
}

/** Why there is nothing to show for an event that names a statement. */
function missing(
  event: ChatEvent,
  payloads: PayloadCapture,
  captureStart: number | null,
): HTMLElement {
  if (payloads.level === "off" || captureStart === null)
    return gap(
      "Payload capture is off. Choose wire or decrypted above to keep what travels from now on. Bytes are not kept while it is off, so this event stays metadata only.",
    );
  if (event.seq < captureStart)
    return gap(
      "This statement travelled before capture was turned on. Its bytes were not kept and cannot be recovered.",
    );
  return gap(
    payloads.losses().evicted > 0
      ? "Not held: evicted to stay within the capture budget."
      : "Not held: the engine handed no bytes for this statement.",
  );
}

/** The layers of one event, or null for an event that carries nothing. */
export function renderTransport(
  event: ChatEvent,
  payloads: PayloadCapture,
  captureStart: number | null,
  chosen: Layer,
  onChoose: (layer: Layer) => void,
): HTMLElement | null {
  const hash = statementOf(event);
  const subscription = event.type === "subscription" ? event : null;
  if (hash === undefined && subscription === null) return null;

  const section = element("section", "cdv-transport");
  section.append(element("h3", "cdv-transport-title", "Transport"));
  const statement =
    hash === undefined ? undefined : payloads.statementFor(hash);
  const frames =
    subscription !== null
      ? payloads.subscriptionFrames(subscription.topicIds, subscription.action)
      : payloads.framesFor(hash!);

  if (subscription !== null) {
    if (frames.length === 0)
      section.append(missing(event, payloads, captureStart));
    else
      section.append(
        badge(
          "public",
          "Public: the subscription's JSON-RPC exchange, matched by the topics its filter names. A subscription carries no statement, so it has no decrypted or decoded layer.",
        ),
        ...frames.map((frame) => frameBlock(frame, null)),
      );
    return section;
  }
  if (statement === undefined) {
    section.append(missing(event, payloads, captureStart));
    return section;
  }

  const tabs = element("div", "cdv-layer-tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "Transport layer");
  const panel = element("div", "cdv-layer");
  panel.setAttribute("role", "tabpanel");
  panel.id = `chat-diag-layer-${event.seq}`;
  const buttons = LAYERS.map(({ layer, label }) => {
    const tab = element("button", "ins-chan", label);
    tab.type = "button";
    tab.setAttribute("role", "tab");
    tab.dataset.layer = layer;
    tab.setAttribute("aria-controls", panel.id);
    tabs.append(tab);
    return tab;
  });
  const show = (layer: Layer) => {
    for (const tab of buttons) {
      const active = tab.dataset.layer === layer;
      tab.classList.toggle("active", active);
      tab.setAttribute("aria-selected", String(active));
      tab.tabIndex = active ? 0 : -1;
    }
    panel.replaceChildren(
      ...(layer === "raw"
        ? rawLayer(statement, frames)
        : layer === "decrypted"
          ? decryptedLayer(statement)
          : decodedLayer(statement)),
    );
  };
  tabs.addEventListener("click", (click) => {
    const tab = (click.target as Element).closest<HTMLElement>("[data-layer]");
    if (tab === null) return;
    onChoose(tab.dataset.layer as Layer);
    show(tab.dataset.layer as Layer);
  });
  tabs.addEventListener("keydown", (key) => {
    if (key.key !== "ArrowLeft" && key.key !== "ArrowRight") return;
    const index = buttons.findIndex((tab) => tab.classList.contains("active"));
    const next =
      buttons[
        (index + (key.key === "ArrowRight" ? 1 : buttons.length - 1)) %
          buttons.length
      ];
    key.preventDefault();
    onChoose(next.dataset.layer as Layer);
    show(next.dataset.layer as Layer);
    next.focus();
  });
  show(chosen);
  section.append(tabs, panel);
  return section;
}
