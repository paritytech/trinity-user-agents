import {
  INSPECTOR_LAYOUT_CSS,
  INSPECTOR_SHELL_CSS,
  TRACE_DETAIL_CSS,
} from "@parity/truapi-debugger";
import "./chat-diagnostics-view.css";
import type { CaptureLevel } from "./capture.js";
import {
  renderTransport,
  transportCopyText,
  type Layer,
} from "./chat-transport-view.js";
import {
  EVENT_TYPE_LABELS,
  eventDirection,
  eventFields,
  eventOutcome,
  eventRecord,
  eventSearchText,
  eventSummary,
  isProblem,
  kindLabel,
  outcomeWord,
  relatedKeys,
  type ChatDiagnostics,
  type ChatEvent,
  type EventDirection,
  type EventOutcome,
  type RelatedBy,
} from "./diagnostics.js";
import { shortHex } from "../../src/chat/nova/events.js";

/**
 * The inspector's Chat view: the chat engine's events. It is a separate list
 * from the TrUAPI traces, because none of these events is a TrUAPI frame, but
 * it uses the TrUAPI inspector's shell and styles: a filter bar, a summary
 * strip, an event list beside the selected event's detail, and a status line.
 * It redraws only while shown.
 *
 * Filters only choose which recorded events are shown. Pausing freezes the
 * list at the newest event so far; recording goes on, and the events that
 * arrive meanwhile are counted. When the recorded events are cleared, which
 * happens when the session changes, the selection, account or id filter and
 * pause go with them.
 */

const DIRECTIONS: Record<
  EventDirection,
  { glyph: string; role: string; chip: string; text: string }
> = {
  in: {
    glyph: "◀",
    role: "received",
    chip: "received",
    text: "Received from the statement store",
  },
  out: {
    glyph: "▶",
    role: "sent",
    chip: "sent",
    text: "Sent to the statement store",
  },
  read: {
    glyph: "◆",
    role: "read",
    chip: "reads",
    text: "Read from chain storage",
  },
  engine: { glyph: "●", role: "engine", chip: "engine", text: "Chat engine" },
};

const OUTCOMES: Record<EventOutcome, string> = {
  ok: "Succeeded",
  info: "Reported",
  warning: "Set aside or refused",
  error: "Failed",
};

const RELATED_NAMES: Record<RelatedBy, string> = {
  account: "account",
  id: "request id",
  statement: "statement",
};

/** A related value as a button shows it: statement hashes are shortened, the filter keeps them whole. */
const relatedLabel = (by: RelatedBy, value: string): string =>
  by === "statement" ? shortHex(value) : value;

const TYPES = Object.keys(EVENT_TYPE_LABELS) as ChatEvent["type"][];
const DIRECTION_KEYS = Object.keys(DIRECTIONS) as EventDirection[];
const TOP_ACCOUNTS = 4;

/** The TrUAPI inspector's shared rules, scoped to this view. */
const SHARED_CSS = `.chat-diagnostics {\n${INSPECTOR_SHELL_CSS}\n${TRACE_DETAIL_CSS}\n${INSPECTOR_LAYOUT_CSS}\n}`;

const pad = (value: number, width = 2): string =>
  String(value).padStart(width, "0");

function clock(at: number, withMs = false): string {
  const date = new Date(at);
  const seconds = `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  return withMs ? `${seconds}.${pad(date.getMilliseconds(), 3)}` : seconds;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function elapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.round((ms % 60_000) / 1000)}s`;
}

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

function button(className: string, text: string): HTMLButtonElement {
  const node = element("button", className, text);
  node.type = "button";
  return node;
}

/** A badge in the debugger's palette: green, amber or red. */
function badge(event: ChatEvent): HTMLSpanElement | null {
  const word = outcomeWord(event);
  if (word === null) return null;
  const node = element(
    "span",
    `td-badge cdv-badge-${eventOutcome(event)}`,
    word,
  );
  node.title = OUTCOMES[eventOutcome(event)];
  return node;
}

function directionGlyph(event: ChatEvent, className: string): HTMLSpanElement {
  const direction = eventDirection(event);
  const glyph = element(
    "span",
    `${className} cdv-dir-${direction}`,
    DIRECTIONS[direction].glyph,
  );
  glyph.title = DIRECTIONS[direction].text;
  return glyph;
}

/** A statistic in the summary strip. */
function stat(
  key: string,
  value: number,
  sub: string,
  tone: "plain" | "warn" | "good" = "plain",
): HTMLDivElement {
  const tile = element(
    "div",
    [
      "ins-stat",
      tone === "plain" ? "" : tone,
      value === 0 && tone !== "plain" ? "zero" : "",
    ]
      .filter(Boolean)
      .join(" "),
  );
  const number = element("span", "n", String(value));
  if (sub !== "") number.append(" ", element("span", "sub", sub));
  tile.append(number, element("span", "k", key));
  return tile;
}

interface Row {
  event: ChatEvent;
  search: string;
  item: HTMLDivElement;
}

function createRow(event: ChatEvent): Row {
  const item = element("div", "td-op cdv-op chat-diag-row");
  item.id = `chat-diag-event-${event.seq}`;
  item.dataset.seq = String(event.seq);
  item.dataset.type = event.type;
  item.dataset.outcome = eventOutcome(event);
  item.setAttribute("role", "option");
  item.setAttribute("aria-selected", "false");
  const summary = element("span", "td-op-method", eventSummary(event));
  summary.title = summary.textContent!;
  const badges = element("span", "td-op-badges");
  const mark = badge(event);
  if (mark !== null) badges.append(mark);
  const arrival = "arrival" in event ? event.arrival : undefined;
  if (arrival !== undefined && arrival > 1) {
    const repeat = element(
      "span",
      "td-badge cdv-badge-repeat",
      `repeat ×${arrival}`,
    );
    repeat.title = `This statement has arrived ${arrival} times in this session`;
    badges.append(repeat);
  }
  const when = element("time", "td-op-meta", clock(event.at));
  when.dateTime = new Date(event.at).toISOString();
  item.append(
    directionGlyph(event, "td-op-kind"),
    element("span", "cdv-op-type", EVENT_TYPE_LABELS[event.type]),
    summary,
    badges,
    when,
  );
  return { event, search: eventSearchText(event), item };
}

function frame(
  glyph: HTMLElement,
  role: string,
  roleClass: string,
  text: string,
): HTMLDivElement {
  const row = element("div", "td-frame");
  const meta = element("div", "td-frame-meta");
  meta.append(
    glyph,
    element("span", `td-frame-role ${roleClass}`, role),
    element("span", "td-frame-method", text),
  );
  row.append(meta);
  return row;
}

const ROLE_CLASSES: Record<EventDirection, string> = {
  in: "td-role-response",
  out: "td-role-request",
  read: "td-role-receive",
  engine: "",
};

function createDetail(
  event: ChatEvent,
  previous: ChatEvent | undefined,
  transport: HTMLElement | null,
): HTMLDivElement {
  const detail = element("div", "cdv-detail");
  const direction = eventDirection(event);

  const drilldown = element("div", "td-drilldown");
  const head = element("div", "td-trace-head");
  const badges = element("span", "td-trace-badges");
  const mark = badge(event);
  if (mark !== null) badges.append(mark);
  head.append(
    element(
      "span",
      "td-trace-id",
      `#${event.seq} · ${EVENT_TYPE_LABELS[event.type]}`,
    ),
    element(
      "span",
      "td-trace-meta",
      previous === undefined
        ? clock(event.at, true)
        : `${clock(event.at, true)} · ${elapsed(event.at - previous.at)} after #${previous.seq}`,
    ),
    badges,
  );
  const frames = element("div", "td-frames");
  frames.append(
    frame(
      directionGlyph(event, "td-frame-dir"),
      DIRECTIONS[direction].role,
      ROLE_CLASSES[direction],
      eventSummary(event),
    ),
  );
  if (event.type === "session" && event.kinds !== undefined)
    event.kinds.forEach((kind, index) =>
      frames.append(
        frame(
          directionGlyph(event, "td-frame-dir"),
          `message ${index + 1}`,
          kind === null ? "td-role-malformed" : "",
          kindLabel(kind),
        ),
      ),
    );
  drilldown.append(head, frames);

  const fields = element("dl", "cdv-fields");
  const define = (term: string, value: string) =>
    fields.append(
      element("dt", undefined, term),
      element("dd", undefined, value),
    );
  define("direction", DIRECTIONS[direction].text);
  for (const { label, value } of eventFields(event))
    define(label.toLowerCase(), value);

  const actions = element("div", "cdv-actions");
  for (const [by, value] of Object.entries(relatedKeys(event)) as [
    RelatedBy,
    string,
  ][]) {
    const filter = button(
      "ins-method",
      `only ${RELATED_NAMES[by]} ${relatedLabel(by, value)}`,
    );
    filter.dataset.relatedBy = by;
    filter.dataset.relatedValue = value;
    filter.append(" ", element("b", "cdv-n"));
    actions.append(filter);
  }
  const copy = button("ins-method", "copy event");
  copy.dataset.copyEvent = String(event.seq);
  actions.append(copy);

  const record = element(
    "pre",
    "td-detail-pre",
    JSON.stringify(eventRecord(event).event, null, 2),
  );
  record.setAttribute("aria-label", "Event metadata");

  detail.append(
    drilldown,
    fields,
    actions,
    ...(transport ? [transport] : []),
    record,
  );
  return detail;
}

export interface ChatDiagnosticsView {
  show(visible: boolean): void;
}

export function mountChatDiagnostics(
  root: HTMLElement,
  diagnostics: ChatDiagnostics,
): ChatDiagnosticsView {
  const shared = element("style");
  shared.textContent = SHARED_CSS;
  document.head.prepend(shared);

  const top = element("div", "ins-top");
  const title = element("span", "ins-title", "Chat ");
  title.append(element("span", "accent", "Event Inspector"));
  const filter = element("input", "ins-filter");
  filter.type = "search";
  filter.placeholder = "filter events…";
  filter.title =
    "Filter by id, account, kind name or number, or text. Press / to come back here.";
  filter.autocomplete = "off";
  filter.spellcheck = false;
  filter.setAttribute("aria-label", "Filter chat events");
  const typeSelect = element("select", "ins-sort");
  typeSelect.setAttribute("aria-label", "Event type");
  typeSelect.title = "Show one type of event";
  const allTypes = element("option", undefined, "all types");
  allTypes.value = "";
  typeSelect.append(allTypes);
  for (const type of TYPES) {
    const option = element("option", undefined, EVENT_TYPE_LABELS[type]);
    option.value = type;
    typeSelect.append(option);
  }
  const captureSelect = element("select", "ins-sort cdv-capture");
  captureSelect.setAttribute("aria-label", "Payload capture");
  captureSelect.title =
    "Keep what chat sends and receives, from now on, in this tab only. Off by default and again after a session change.";
  for (const [value, label] of [
    ["off", "payloads: off"],
    ["wire", "payloads: wire"],
    ["decrypted", "payloads: wire + decrypted"],
  ] as const) {
    const option = element("option", undefined, label);
    option.value = value;
    captureSelect.append(option);
  }
  const channels = element("span", "ins-channels");
  channels.setAttribute("role", "group");
  channels.setAttribute("aria-label", "Show events by direction");
  const chip = (text: string, live: boolean) => {
    const node = button("ins-chan", "");
    node.append(element("span", live ? "dot live" : "dot"), text);
    return node;
  };
  const allChip = chip("all", true);
  const directionChips = new Map(
    DIRECTION_KEYS.map((direction) => [
      direction,
      chip(DIRECTIONS[direction].chip, false),
    ]),
  );
  const problemsChip = chip("problems", false);
  problemsChip.classList.add("cdv-problems");
  const relatedChip = button("ins-chan active cdv-related", "");
  relatedChip.title = "Show all events again";
  channels.append(
    allChip,
    ...directionChips.values(),
    problemsChip,
    relatedChip,
  );
  const tools = element("span", "cdv-tools");
  const follow = button("ins-chan cdv-follow", "");
  follow.title =
    "Keep the newest event in view. Pausing freezes this list only; chat and recording go on.";
  const copyShown = button("ins-chan", "copy");
  copyShown.title = "Copy the shown events as JSON (metadata only)";
  const clear = button("ins-chan", "clear");
  clear.title =
    "Forget the recorded events and payloads; capture stays as chosen";
  tools.append(follow, copyShown, clear);
  top.append(title, filter, typeSelect, captureSelect, channels, tools);

  const summary = element("div", "ins-summary empty");
  const bodyEl = element("div", "ins-body");
  const list = element("div", "ins-list");
  list.tabIndex = 0;
  list.setAttribute("role", "listbox");
  list.setAttribute("aria-label", "Chat events, oldest first");
  const split = element("div", "ins-split");
  split.tabIndex = 0;
  split.setAttribute("role", "separator");
  split.setAttribute("aria-orientation", "vertical");
  split.setAttribute("aria-label", "Resize the event list");
  const detailPane = element("div", "ins-detail");
  detailPane.tabIndex = 0;
  detailPane.setAttribute("aria-label", "Selected chat event");
  detailPane.setAttribute("aria-live", "polite");
  const listEmpty = element("div", "td-op-empty");
  bodyEl.append(list, split, detailPane);
  const status = element("div", "ins-status");
  const statusCount = element("span");
  const statusFollow = element("span");
  const newer = button("cdv-newer", "");
  const statusNote = element("span");
  const statusEvicted = element("span", "mismatch");
  const announcer = element("span", "cdv-unseen");
  announcer.setAttribute("aria-live", "polite");
  status.append(
    statusCount,
    statusFollow,
    newer,
    statusNote,
    statusEvicted,
    announcer,
  );
  root.append(top, summary, bodyEl, status);

  let visible = false;
  let query = "";
  let typeFilter: ChatEvent["type"] | "" = "";
  let directionFilter: EventDirection | null = null;
  let problemsOnly = false;
  let related: { by: RelatedBy; value: string } | null = null;
  let selected: number | null = null;
  let shownDetail: number | null = null;
  let shownGeneration = -1;
  let layer: Layer = "raw";
  /** The newest event shown while paused; null while following. */
  let pausedAt: number | null = null;
  let lastSeq = 0;
  const rows = new Map<number, Row>();
  /** Rows that pass the filters, in the order drawn. */
  let drawn: Row[] = [];

  const shownEvents = (): readonly ChatEvent[] => {
    const events = diagnostics.list();
    return pausedAt === null
      ? events
      : events.filter((event) => event.seq <= pausedAt!);
  };

  const rowFor = (event: ChatEvent): Row => {
    let row = rows.get(event.seq);
    if (row === undefined) {
      row = createRow(event);
      rows.set(event.seq, row);
    }
    return row;
  };

  const passes = (row: Row): boolean =>
    (typeFilter === "" || row.event.type === typeFilter) &&
    (directionFilter === null ||
      eventDirection(row.event) === directionFilter) &&
    (!problemsOnly || isProblem(row.event)) &&
    (query === "" || row.search.includes(query)) &&
    (related === null || relatedKeys(row.event)[related.by] === related.value);

  /** Follow the recorded list: forget what it no longer holds. */
  const sync = () => {
    const events = diagnostics.list();
    const first = events[0];
    if (first === undefined || first.seq > lastSeq) {
      selected = null;
      related = null;
      pausedAt = null;
    }
    lastSeq = events.at(-1)?.seq ?? lastSeq;
    const live = new Set(events.map((event) => event.seq));
    for (const [seq, row] of rows)
      if (!live.has(seq)) {
        row.item.remove();
        rows.delete(seq);
      }
    if (selected !== null && !live.has(selected)) selected = null;
  };

  const placeRows = (wanted: HTMLElement[]) => {
    const keep = new Set<Element>(wanted);
    for (const child of [...list.children])
      if (!keep.has(child)) child.remove();
    let cursor = list.firstElementChild;
    for (const item of wanted)
      if (item === cursor) cursor = cursor.nextElementSibling;
      else list.insertBefore(item, cursor);
  };

  const drawDetail = () => {
    const generation = diagnostics.payloads.generation;
    if (selected === shownDetail && generation === shownGeneration) return;
    const sameEvent = selected === shownDetail;
    shownDetail = selected;
    shownGeneration = generation;
    const events = diagnostics.list();
    const index = events.findIndex((event) => event.seq === selected);
    const scroll = detailPane.scrollTop;
    detailPane.replaceChildren(
      index === -1
        ? element(
            "div",
            "td-detail-empty",
            "Select an event to inspect it. ↑ ↓ move, Esc closes.",
          )
        : createDetail(
            events[index],
            events[index - 1],
            renderTransport(
              events[index],
              diagnostics.payloads,
              diagnostics.captureStart(),
              layer,
              (chosen) => (layer = chosen),
            ),
          ),
    );
    detailPane.scrollTop = sameEvent ? scroll : 0;
  };

  const drawSummary = (shown: readonly ChatEvent[]) => {
    const count = (test: (event: ChatEvent) => boolean) =>
      shown.filter(test).length;
    const received = count((event) => eventDirection(event) === "in");
    const setAside = count((event) => event.type === "set-aside");
    const sent = count((event) => event.type === "submit");
    const stored = count(
      (event) => event.type === "submit" && event.result === "stored",
    );
    const reads = count((event) => event.type === "lookup");
    const found = count(
      (event) => event.type === "lookup" && event.outcome === "found",
    );
    const problems = count(isProblem);
    const errors = count((event) => eventOutcome(event) === "error");
    const filtered = drawn.length !== shown.length;
    const accounts = new Map<string, number>();
    for (const event of shown) {
      const account = relatedKeys(event).account;
      if (account !== undefined)
        accounts.set(account, (accounts.get(account) ?? 0) + 1);
    }
    const pills = element("span", "ins-methods");
    pills.setAttribute("aria-label", "Accounts with the most events");
    for (const [account, total] of [...accounts]
      .sort((a, b) => b[1] - a[1])
      .slice(0, TOP_ACCOUNTS)) {
      const pill = button("ins-method", `${account} `);
      pill.dataset.relatedBy = "account";
      pill.dataset.relatedValue = account;
      pill.title = `Show only events for account ${account}`;
      pill.setAttribute(
        "aria-pressed",
        String(related?.by === "account" && related.value === account),
      );
      pill.append(element("b", undefined, String(total)));
      pills.append(pill);
    }
    summary.className = `ins-summary${shown.length === 0 ? " empty" : ""}`;
    summary.replaceChildren(
      ...(shown.length === 0
        ? [
            element(
              "span",
              undefined,
              "No chat events yet. Metadata only: no keys, message text or tokens.",
            ),
          ]
        : [
            stat(
              "events",
              shown.length,
              filtered ? `${drawn.length} shown` : "",
            ),
            stat(
              "received",
              received,
              setAside > 0 ? `${setAside} set aside` : "",
            ),
            stat("sent", sent, sent > 0 ? `${stored} stored` : ""),
            stat("key reads", reads, reads > 0 ? `${found} found` : ""),
            stat(
              "problems",
              problems,
              errors > 0 ? `${errors} error${errors === 1 ? "" : "s"}` : "",
              "warn",
            ),
            stat("evicted", diagnostics.dropped(), "", "warn"),
            pills,
          ]),
    );
  };

  const draw = () => {
    if (!visible) return;
    const atEnd = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
    const shown = shownEvents();
    const all = diagnostics.list();
    drawn = shown.map(rowFor).filter(passes);
    placeRows(drawn.length === 0 ? [listEmpty] : drawn.map((row) => row.item));
    listEmpty.textContent =
      shown.length === 0
        ? "waiting for chat events…"
        : "no events match the filter";
    for (const row of rows.values()) {
      const isSelected = row.event.seq === selected;
      row.item.classList.toggle("selected", isSelected);
      row.item.setAttribute("aria-selected", String(isSelected));
    }
    if (selected !== null && rows.has(selected))
      list.setAttribute("aria-activedescendant", `chat-diag-event-${selected}`);
    else list.removeAttribute("aria-activedescendant");
    drawDetail();
    for (const filterButton of detailPane.querySelectorAll<HTMLElement>(
      "[data-related-by]",
    )) {
      const by = filterButton.dataset.relatedBy as RelatedBy;
      const value = filterButton.dataset.relatedValue;
      filterButton.querySelector(".cdv-n")!.textContent = String(
        shown.filter((event) => relatedKeys(event)[by] === value).length,
      );
      filterButton.setAttribute(
        "aria-pressed",
        String(related?.by === by && related.value === value),
      );
    }
    drawSummary(shown);

    const noFilter = directionFilter === null && !problemsOnly;
    allChip.classList.toggle("active", noFilter);
    allChip.setAttribute("aria-pressed", String(noFilter));
    for (const [direction, node] of directionChips) {
      node.classList.toggle("active", directionFilter === direction);
      node.setAttribute("aria-pressed", String(directionFilter === direction));
    }
    problemsChip.classList.toggle("active", problemsOnly);
    problemsChip.setAttribute("aria-pressed", String(problemsOnly));
    relatedChip.hidden = related === null;
    if (related !== null)
      relatedChip.textContent = `${RELATED_NAMES[related.by]} ${relatedLabel(related.by, related.value)} ✕`;

    follow.classList.toggle("active", pausedAt === null);
    follow.setAttribute("aria-pressed", String(pausedAt === null));
    follow.replaceChildren(
      element("span", "dot live"),
      pausedAt === null ? "following" : "paused",
    );
    copyShown.disabled = drawn.length === 0;
    clear.disabled = all.length === 0;

    statusCount.textContent = `${shown.length} event${shown.length === 1 ? "" : "s"}`;
    statusFollow.className = pausedAt === null ? "live" : "cdv-paused";
    statusFollow.textContent =
      pausedAt === null ? "following" : `paused at #${pausedAt}`;
    const arrived = all.length - shown.length;
    newer.hidden = pausedAt === null || arrived === 0;
    newer.textContent = `${arrived} new · resume`;
    const evicted = diagnostics.dropped();
    statusEvicted.hidden = evicted === 0;
    statusEvicted.textContent = `${evicted} evicted`;
    const payloads = diagnostics.payloads;
    captureSelect.value = payloads.level;
    captureSelect.classList.toggle("cdv-capturing", payloads.level !== "off");
    if (payloads.level === "off") {
      statusNote.className = "";
      statusNote.textContent = "metadata only · this session";
    } else {
      const usage = payloads.usage();
      const losses = payloads.losses();
      statusNote.className =
        payloads.level === "decrypted" ? "cdv-capture-decrypted" : "live";
      statusNote.textContent = [
        `capturing ${payloads.level === "decrypted" ? "wire + decrypted" : "wire"}`,
        `${usage.frames} frames · ${usage.statements} statements`,
        `${formatBytes(usage.bytes)} of ${formatBytes(usage.budget)}`,
        losses.evicted > 0 ? `${losses.evicted} evicted` : "",
        losses.truncated > 0 ? `${losses.truncated} cut` : "",
      ]
        .filter(Boolean)
        .join(" · ");
    }

    if (pausedAt === null && atEnd) list.scrollTop = list.scrollHeight;
  };

  diagnostics.onChange(() => {
    sync();
    draw();
  });

  const resume = () => {
    pausedAt = null;
    draw();
    list.scrollTop = list.scrollHeight;
  };

  const select = (seq: number | null) => {
    selected = seq;
    draw();
    if (seq !== null) rows.get(seq)?.item.scrollIntoView({ block: "nearest" });
  };

  const toggleRelated = (by: RelatedBy, value: string) => {
    related =
      related?.by === by && related.value === value ? null : { by, value };
    draw();
  };

  let announceTimer: ReturnType<typeof setTimeout> | undefined;
  const announce = (text: string) => {
    announcer.textContent = text;
    clearTimeout(announceTimer);
    announceTimer = setTimeout(() => (announcer.textContent = ""), 2000);
  };

  const copy = (payload: unknown, source: HTMLButtonElement) =>
    copyText(JSON.stringify(payload, null, 2), source);
  const copyText = async (text: string, source: HTMLButtonElement) => {
    const label = source.firstChild!;
    const original = label.textContent;
    let result: string;
    try {
      await navigator.clipboard.writeText(text);
      result = "copied";
    } catch {
      result = "copy failed";
    }
    label.textContent = result;
    announce(result);
    setTimeout(() => (label.textContent = original), 1500);
  };

  filter.addEventListener("input", () => {
    query = filter.value.trim().toLowerCase();
    draw();
  });
  filter.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && filter.value !== "") {
      event.stopPropagation();
      filter.value = query = "";
      draw();
    }
  });
  typeSelect.addEventListener("change", () => {
    typeFilter = typeSelect.value as ChatEvent["type"] | "";
    draw();
  });
  allChip.addEventListener("click", () => {
    directionFilter = null;
    problemsOnly = false;
    draw();
  });
  for (const [direction, node] of directionChips)
    node.addEventListener("click", () => {
      directionFilter = directionFilter === direction ? null : direction;
      draw();
    });
  problemsChip.addEventListener("click", () => {
    problemsOnly = !problemsOnly;
    draw();
  });
  relatedChip.addEventListener("click", () => {
    related = null;
    draw();
  });
  follow.addEventListener("click", () => {
    if (pausedAt === null) {
      pausedAt = lastSeq;
      draw();
    } else resume();
  });
  newer.addEventListener("click", resume);
  copyShown.addEventListener(
    "click",
    () =>
      void copy(
        drawn.map((row) => eventRecord(row.event)),
        copyShown,
      ),
  );
  clear.addEventListener("click", () => diagnostics.forget());
  captureSelect.addEventListener("change", () =>
    diagnostics.payloads.setLevel(captureSelect.value as CaptureLevel),
  );

  const relatedClick = (event: Event): boolean => {
    const target = (event.target as Element).closest<HTMLButtonElement>(
      "[data-related-by]",
    );
    if (target === null) return false;
    toggleRelated(
      target.dataset.relatedBy as RelatedBy,
      target.dataset.relatedValue!,
    );
    return true;
  };
  summary.addEventListener("click", relatedClick);
  detailPane.addEventListener("click", (event) => {
    if (relatedClick(event)) return;
    const layerCopy = (event.target as Element).closest<HTMLButtonElement>(
      "[data-copy-layer]",
    );
    const text = layerCopy === null ? undefined : transportCopyText(layerCopy);
    if (layerCopy !== null && text !== undefined) {
      void copyText(text, layerCopy);
      return;
    }
    const source = (event.target as Element).closest<HTMLButtonElement>(
      "[data-copy-event]",
    );
    const row = source && rows.get(Number(source.dataset.copyEvent));
    if (source && row) void copy(eventRecord(row.event), source);
  });

  list.addEventListener("click", (event) => {
    const item = (event.target as Element).closest<HTMLElement>(".cdv-op");
    if (item === null) return;
    list.focus({ preventScroll: true });
    select(Number(item.dataset.seq));
  });

  list.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      if (selected !== null) select(null);
      else if (related !== null) {
        related = null;
        draw();
      } else return;
      event.preventDefault();
      return;
    }
    const index = drawn.findIndex((row) => row.event.seq === selected);
    const next =
      event.key === "ArrowDown"
        ? drawn[index === -1 ? 0 : index + 1]
        : event.key === "ArrowUp"
          ? drawn[index === -1 ? drawn.length - 1 : index - 1]
          : event.key === "Home"
            ? drawn[0]
            : event.key === "End"
              ? drawn.at(-1)
              : undefined;
    if (next === undefined) return;
    event.preventDefault();
    select(next.event.seq);
  });

  root.addEventListener("keydown", (event) => {
    const target = event.target as Element;
    if (
      event.key === "/" &&
      !event.metaKey &&
      !event.ctrlKey &&
      !(target instanceof HTMLInputElement) &&
      !(target instanceof HTMLSelectElement)
    ) {
      event.preventDefault();
      filter.focus();
      filter.select();
    }
  });

  const MIN_COLUMN = 180;
  const resizeList = (width: number) => {
    const max = bodyEl.clientWidth - MIN_COLUMN;
    root.style.setProperty(
      "--list-w",
      `${Math.round(Math.max(MIN_COLUMN, Math.min(max, width)))}px`,
    );
  };
  split.addEventListener("pointerdown", (event) => {
    split.setPointerCapture(event.pointerId);
    const left = bodyEl.getBoundingClientRect().left;
    const move = (moved: PointerEvent) => resizeList(moved.clientX - left);
    const stop = () => {
      split.removeEventListener("pointermove", move);
      split.removeEventListener("pointerup", stop);
      split.removeEventListener("pointercancel", stop);
    };
    split.addEventListener("pointermove", move);
    split.addEventListener("pointerup", stop);
    split.addEventListener("pointercancel", stop);
  });
  split.addEventListener("keydown", (event) => {
    const step =
      event.key === "ArrowLeft" ? -24 : event.key === "ArrowRight" ? 24 : 0;
    if (step === 0) return;
    event.preventDefault();
    resizeList(list.getBoundingClientRect().width + step);
  });

  sync();
  return {
    show(next) {
      const opening = next && !visible;
      visible = next;
      root.hidden = !next;
      draw();
      if (opening && pausedAt === null) list.scrollTop = list.scrollHeight;
    },
  };
}
