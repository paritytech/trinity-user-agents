import "./activity-log.css";
import type { ActivityCategory } from "../../src/platform/activity.js";
import { copyInPage } from "../../src/app/copy.js";
import {
  CATEGORY_LABELS,
  clock,
  eventText,
  matches,
  type ActivityEvent,
  type ActivityFilter,
  type ActivityLog,
} from "./log.js";

/**
 * The inspector's Activity view: one compact row per event, oldest first, which
 * opens to the event's details. Filters, search and pausing choose only what
 * the list shows. Recording goes on while paused, and the log still drops its
 * oldest events past its limit, so a paused list holds no more than the log.
 *
 * Rows are kept by event id and only added or removed, never redrawn, so a
 * row keeps its focus and its open state while events arrive.
 */

const CATEGORIES = Object.keys(CATEGORY_LABELS) as ActivityCategory[];

interface Row {
  item: HTMLLIElement;
  head: HTMLElement;
  details: HTMLDListElement | null;
}

function node<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const created = document.createElement(tag);
  if (className !== undefined) created.className = className;
  if (text !== undefined) created.textContent = text;
  return created;
}

function chip(text: string, label: string): HTMLButtonElement {
  const button = node("button", "activity-chip", text);
  button.type = "button";
  button.title = label;
  button.setAttribute("aria-pressed", "false");
  return button;
}

export interface ActivityView {
  show(visible: boolean): void;
}

/**
 * Mount the Activity view's controls around `list`, the `<ol>` its rows go
 * in, inside `root`, the inspector panel that holds it.
 */
export function mountActivityLog(
  root: HTMLElement,
  list: HTMLOListElement,
  log: ActivityLog,
): ActivityView {
  root.classList.add("activity");
  list.classList.add("activity-list");
  list.setAttribute("aria-label", "Host activity, oldest first");

  const tools = node("div", "activity-tools");
  const search = node("input", "activity-search");
  search.type = "search";
  search.placeholder = "Search product or text";
  search.autocomplete = "off";
  search.spellcheck = false;
  search.setAttribute("aria-label", "Search the log");
  const chips = node("div", "activity-chips");
  chips.setAttribute("role", "group");
  chips.setAttribute("aria-label", "Show events by category");
  const allChip = chip("All", "Show every category");
  const categoryChips = new Map(
    CATEGORIES.map((category) => [
      category,
      chip(
        CATEGORY_LABELS[category],
        `Show ${CATEGORY_LABELS[category]} events only`,
      ),
    ]),
  );
  const errorsChip = chip("Errors", "Show only what failed or was refused");
  errorsChip.classList.add("activity-errors");
  chips.append(allChip, ...categoryChips.values(), errorsChip);
  const actions = node("div", "activity-actions");
  const follow = node("button", "activity-chip activity-follow");
  follow.type = "button";
  const copy = node("button", "activity-chip", "Copy");
  copy.type = "button";
  copy.title = "Copy the events shown, with their details";
  const clear = node("button", "activity-chip", "Clear");
  clear.type = "button";
  clear.title = "Forget every event logged so far";
  actions.append(follow, copy, clear);
  tools.append(search, actions, chips);

  const empty = node("p", "activity-empty");
  const status = node("p", "activity-status");
  const statusText = node("span");
  const newer = node("button", "activity-newer");
  newer.type = "button";
  status.append(statusText, newer);
  const announcer = node("span", "activity-announce");
  announcer.setAttribute("role", "status");
  root.insertBefore(tools, list);
  list.after(empty, status, announcer);

  const filter: ActivityFilter = {
    category: null,
    failedOnly: false,
    query: "",
  };
  /** The newest event shown while paused; null while following. */
  let pausedAt: number | null = null;
  const rows = new Map<number, Row>();
  const open = new Set<number>();
  let scheduled = false;
  let shownCount = 0;

  const setOpen = (id: number, row: Row, opened: boolean) => {
    if (row.details === null) return;
    if (opened) open.add(id);
    else open.delete(id);
    row.head.setAttribute("aria-expanded", String(opened));
    row.details.hidden = !opened;
    row.item.classList.toggle("open", opened);
  };

  const createRow = (event: ActivityEvent): Row => {
    const item = node("li", "activity-row");
    item.dataset.id = String(event.id);
    item.dataset.category = event.category;
    if (event.failed === true) item.dataset.failed = "true";
    const expandable = event.details !== undefined;
    const head = expandable
      ? node("button", "activity-head")
      : node("div", "activity-head");
    const time = node("time", "activity-time", clock(event.at));
    time.dateTime = new Date(event.at).toISOString();
    const text = node("span", "activity-text");
    text.append(node("span", "activity-action", event.action));
    if (event.subject !== undefined)
      text.append(" ", node("span", "activity-subject", event.subject));
    if (event.note !== undefined)
      text.append(node("span", "activity-note", event.note));
    head.append(
      time,
      node("span", "activity-category", CATEGORY_LABELS[event.category]),
      text,
    );
    item.append(head);
    let details: HTMLDListElement | null = null;
    if (expandable) {
      const button = head as HTMLButtonElement;
      button.type = "button";
      details = node("dl", "activity-details");
      details.id = `activity-${event.id}`;
      details.hidden = true;
      for (const { label, value } of event.details!)
        details.append(
          node("dt", undefined, label),
          node("dd", undefined, value),
        );
      button.setAttribute("aria-controls", details.id);
      button.setAttribute("aria-expanded", "false");
      item.append(details);
    }
    const row = { item, head, details };
    if (expandable)
      head.addEventListener("click", () =>
        setOpen(event.id, row, !open.has(event.id)),
      );
    return row;
  };

  const shownEvents = (): ActivityEvent[] =>
    log
      .list()
      .filter(
        (event) =>
          (pausedAt === null || event.id <= pausedAt) && matches(event, filter),
      );

  const render = () => {
    scheduled = false;
    const events = log.list();
    if (pausedAt !== null && events.length === 0) pausedAt = null;
    const live = new Set(events.map((event) => event.id));
    for (const [id, row] of rows)
      if (!live.has(id)) {
        row.item.remove();
        rows.delete(id);
        open.delete(id);
      }
    const shown = shownEvents();
    const wanted = new Set(shown.map((event) => event.id));
    // Rows that stay are never moved: moving a node takes its focus away.
    for (const child of [...list.children])
      if (!wanted.has(Number((child as HTMLElement).dataset.id)))
        child.remove();
    let cursor = list.firstElementChild;
    for (const event of shown) {
      let row = rows.get(event.id);
      if (row === undefined) {
        row = createRow(event);
        rows.set(event.id, row);
      }
      if (row.item === cursor) cursor = cursor.nextElementSibling;
      else list.insertBefore(row.item, cursor);
    }
    shownCount = shown.length;

    allChip.setAttribute("aria-pressed", String(filter.category === null));
    for (const [category, button] of categoryChips)
      button.setAttribute("aria-pressed", String(filter.category === category));
    errorsChip.setAttribute("aria-pressed", String(filter.failedOnly));
    follow.textContent = pausedAt === null ? "Pause" : "Follow";
    follow.title =
      pausedAt === null
        ? "Stop following new events. The host keeps logging."
        : "Show new events again and keep the newest in view";
    follow.setAttribute("aria-pressed", String(pausedAt !== null));
    copy.disabled = shown.length === 0;

    empty.hidden = shown.length > 0;
    empty.textContent =
      events.length === 0 ? "Nothing logged yet." : "No events match.";
    const unseen =
      pausedAt === null
        ? 0
        : events.filter((event) => event.id > pausedAt!).length;
    const dropped = log.dropped();
    statusText.textContent = [
      `${shown.length} of ${events.length} events`,
      pausedAt === null ? "following" : "paused",
      dropped > 0 ? `${dropped} older dropped` : null,
    ]
      .filter((part) => part !== null)
      .join(" · ");
    newer.hidden = unseen === 0;
    newer.textContent = `${unseen} new`;
    newer.title = "Follow again and show them";
    if (pausedAt === null) list.scrollTop = list.scrollHeight;
  };

  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(render);
  };

  /** Draw now and say how many events the filters leave. */
  const refilter = () => {
    render();
    announcer.textContent =
      shownCount === 0 ? "No events match." : `${shownCount} events shown.`;
  };

  log.onChange(schedule);
  search.addEventListener("input", () => {
    filter.query = search.value.trim().toLowerCase();
    refilter();
  });
  allChip.addEventListener("click", () => {
    filter.category = null;
    refilter();
  });
  for (const [category, button] of categoryChips)
    button.addEventListener("click", () => {
      filter.category = filter.category === category ? null : category;
      refilter();
    });
  errorsChip.addEventListener("click", () => {
    filter.failedOnly = !filter.failedOnly;
    refilter();
  });
  const resume = () => {
    pausedAt = null;
    render();
  };
  follow.addEventListener("click", () => {
    if (pausedAt !== null) resume();
    else {
      pausedAt = log.list().at(-1)?.id ?? 0;
      render();
    }
  });
  newer.addEventListener("click", resume);
  copy.addEventListener("click", () => {
    const text = shownEvents().map(eventText).join("\n");
    void copyInPage(text).then((copied) => {
      copy.textContent = copied ? "Copied" : "Copy failed";
      setTimeout(() => (copy.textContent = "Copy"), 1200);
    });
  });
  clear.addEventListener("click", () => {
    pausedAt = null;
    log.clear();
    render();
    announcer.textContent = "Log cleared.";
  });
  render();
  return {
    show(visible) {
      root.hidden = !visible;
      // A list hidden while following could not keep its end in view.
      if (visible && pausedAt === null) list.scrollTop = list.scrollHeight;
    },
  };
}
