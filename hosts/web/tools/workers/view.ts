import "./workers-view.css";
import type {
  WorkerSnapshot,
  WorkerSurface,
} from "../../src/products/product-workers.js";
import { copyInPage } from "../../src/app/copy.js";
import {
  clock,
  lineText,
  passes,
  type ConsoleLine,
  type LevelFilter,
  type WorkerConsoleLog,
} from "./console-log.js";

/**
 * The inspector's Workers view: the product workers this session wants or
 * runs, and for the one selected, its state as the worker manager holds it,
 * and what it wrote to its console. State comes from the manager, never from
 * the console. The console is the worker's own text, kept in this tab's
 * memory for the developer only.
 *
 * Rows are kept by worker name and lines by id, and updated in place, so a
 * focused control keeps its focus while events arrive. It follows changes
 * while hidden too, so it never holds lines its log has forgotten.
 */

/** What the view reads of the worker manager. */
export interface WorkerSource {
  inspect(): WorkerSnapshot[];
  restart(name: string): Promise<void>;
  onChange(listener: () => void): () => void;
}

export interface WorkersViewDeps {
  workers: WorkerSource;
  log: WorkerConsoleLog;
  /**
   * Show the TrUAPI traces of `name`'s worker, narrowed to it where the
   * TrUAPI view can, and saying why where it cannot.
   */
  showTraces(name: string): void;
}

export interface WorkersView {
  show(visible: boolean): void;
  /** Select `name`'s worker. */
  select(name: string): void;
}

const SURFACES: Record<WorkerSurface, string> = {
  chat: "Chat",
  pocket: "Pocket",
};

const FILTERS: { value: LevelFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "warnings", label: "Warnings" },
  { value: "errors", label: "Errors" },
];

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

function button(className: string, text: string, title: string) {
  const created = node("button", className, text);
  created.type = "button";
  created.title = title;
  return created;
}

/** The state's word, as the list and the details show it. */
function stateWord(worker: WorkerSnapshot): string {
  if (worker.state === null) return "Not started";
  if (worker.state.state === "running") return "Running";
  if (worker.state.state === "starting") return "Starting";
  return "Failed";
}

function duration(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/** The facts the details list, in order, from what the manager holds. */
function facts(worker: WorkerSnapshot, now: number): [string, string][] {
  const rows: [string, string][] = [];
  const state = worker.state;
  if (state !== null && worker.since !== null) {
    const age = duration(now - worker.since);
    rows.push([
      state.state === "running"
        ? "Up for"
        : state.state === "starting"
          ? "Starting for"
          : "Failed",
      state.state === "failed" ? `${age} ago, at ${clock(worker.since)}` : age,
    ]);
  }
  rows.push([
    "Runs for",
    worker.surfaces.length === 0
      ? "nothing now"
      : worker.surfaces.map((surface) => SURFACES[surface]).join(", "),
  ]);
  rows.push([
    "Added by",
    worker.addedByHost ? "this host, for every wallet" : "you",
  ]);
  if (worker.run !== null) rows.push(["Run", String(worker.run)]);
  if (state?.state === "running") {
    rows.push(["Record", state.record]);
    rows.push(["Content", state.cid]);
    rows.push(["Version", state.manifest.appVersion.join(".")]);
    rows.push(["Entry", state.manifest.entrypoint]);
  }
  for (const { surface, reason } of worker.refusals)
    rows.push([`Not for ${SURFACES[surface]}`, reason]);
  return rows;
}

export function mountWorkersView(
  root: HTMLElement,
  deps: WorkersViewDeps,
): WorkersView {
  const { workers, log } = deps;

  const top = node("div", "wi-top");
  const title = node("span", "wi-title", "Workers ");
  title.append(node("span", "accent", "Inspector"));
  top.append(title);

  const body = node("div", "wi-body");
  const list = node("ul", "wi-list");
  list.setAttribute("aria-label", "Workers");
  const listEmpty = node(
    "p",
    "wi-empty",
    "No workers. Sign in, then add one in Product → Workers.",
  );
  const listPane = node("div", "wi-list-pane");
  listPane.append(list, listEmpty);

  const detail = node("section", "wi-detail");
  detail.setAttribute("aria-label", "Selected worker");
  const head = node("div", "wi-head");
  const name = node("h3", "wi-name");
  const badge = node("span", "wi-badge");
  const actions = node("div", "wi-actions");
  const traces = button(
    "wi-action",
    "Traces",
    "Show this worker's TrUAPI frames in the TrUAPI view",
  );
  const restart = button(
    "wi-action",
    "Restart",
    "Start this worker again in a new frame. Its earlier run's lines are kept.",
  );
  actions.append(traces, restart);
  head.append(name, badge, actions);
  const problem = node("p", "wi-problem");
  problem.setAttribute("role", "status");
  const factList = node("dl", "wi-facts");
  const note = node("p", "wi-note");
  note.setAttribute("role", "status");

  const consoleHead = node("div", "wi-console-head");
  const consoleTitle = node("h4", undefined, "Console");
  const filters = node("div", "wi-filters");
  filters.setAttribute("role", "group");
  filters.setAttribute("aria-label", "Show console lines by level");
  const filterButtons = new Map(
    FILTERS.map(({ value, label }) => {
      const chip = button(
        "wi-chip",
        label,
        `Show ${label.toLowerCase()} lines`,
      );
      chip.setAttribute("aria-pressed", String(value === "all"));
      return [value, chip] as const;
    }),
  );
  filters.append(...filterButtons.values());
  const copy = button(
    "wi-chip",
    "Copy",
    "Copy the lines shown. They can hold what the product logs.",
  );
  const clear = button("wi-chip", "Clear", "Forget this worker's lines");
  consoleHead.append(consoleTitle, filters, copy, clear);
  const lines = node("ol", "wi-lines");
  lines.setAttribute("aria-label", "Console lines, oldest first");
  lines.tabIndex = 0;
  const linesEmpty = node("p", "wi-empty");
  const status = node("p", "wi-status");
  detail.append(
    head,
    problem,
    factList,
    note,
    consoleHead,
    lines,
    linesEmpty,
    status,
  );
  const pick = node("p", "wi-empty wi-pick", "Choose a worker.");
  body.append(listPane, detail, pick);
  root.append(top, body);

  let selected: string | null = null;
  let filter: LevelFilter = "all";
  let scheduled = false;
  let ticker: ReturnType<typeof setInterval> | undefined;
  const rows = new Map<string, HTMLButtonElement>();
  const drawnLines = new Map<number, HTMLLIElement>();
  let drawnFor: string | null = null;

  const current = (): WorkerSnapshot | null =>
    workers.inspect().find((worker) => worker.name === selected) ?? null;

  const drawList = (snapshot: WorkerSnapshot[]) => {
    const names = new Set(snapshot.map((worker) => worker.name));
    for (const [workerName, row] of rows)
      if (!names.has(workerName)) {
        row.parentElement?.remove();
        rows.delete(workerName);
      }
    let cursor = list.firstElementChild;
    for (const worker of snapshot) {
      let row = rows.get(worker.name);
      if (row === undefined) {
        row = button("wi-row", "", `Inspect ${worker.name}`);
        row.dataset.worker = worker.name;
        row.append(
          node("span", "wi-dot"),
          node("span", "wi-row-name", worker.name),
          node("span", "wi-row-state"),
        );
        row.addEventListener("click", () => select(worker.name));
        rows.set(worker.name, row);
        const item = node("li");
        item.append(row);
        list.insertBefore(item, cursor);
      } else if (cursor !== null && row.parentElement === cursor) {
        cursor = cursor.nextElementSibling;
      } else {
        list.insertBefore(row.parentElement!, cursor);
      }
      const state = worker.state?.state ?? "stopped";
      row.dataset.state = state;
      row.setAttribute("aria-pressed", String(worker.name === selected));
      const errors = log
        .lines(worker.name)
        .filter((line) => line.level === "error").length;
      row.querySelector(".wi-row-state")!.textContent =
        `${stateWord(worker)}${errors > 0 ? ` · ${errors} error${errors === 1 ? "" : "s"}` : ""}`;
    }
    listEmpty.hidden = snapshot.length > 0;
  };

  const drawLines = (workerName: string) => {
    if (drawnFor !== workerName) {
      lines.replaceChildren();
      drawnLines.clear();
      drawnFor = workerName;
    }
    const all = log.lines(workerName);
    const shown = all.filter((line) => passes(line, filter));
    const wanted = new Set(shown.map((line) => line.id));
    for (const [id, item] of drawnLines)
      if (!wanted.has(id)) {
        item.remove();
        drawnLines.delete(id);
      }
    const atEnd = lines.scrollHeight - lines.scrollTop - lines.clientHeight < 8;
    let previousRun: number | null = null;
    let cursor = lines.firstElementChild;
    for (const line of shown) {
      let item = drawnLines.get(line.id);
      if (item === undefined) {
        item = drawLine(line);
        drawnLines.set(line.id, item);
        lines.insertBefore(item, cursor);
      } else cursor = item.nextElementSibling;
      item.dataset.newRun = String(
        previousRun !== null && previousRun !== line.run,
      );
      previousRun = line.run;
    }
    if (atEnd) lines.scrollTop = lines.scrollHeight;
    linesEmpty.hidden = shown.length > 0;
    linesEmpty.textContent =
      all.length === 0
        ? "Nothing written to the console yet."
        : "No lines at this level.";
    const dropped = log.dropped(workerName);
    status.textContent = [
      `${shown.length} of ${all.length} lines`,
      dropped > 0 ? `${dropped} older dropped` : null,
    ]
      .filter((part) => part !== null)
      .join(" · ");
    copy.disabled = shown.length === 0;
    clear.disabled = all.length === 0;
  };

  const drawLine = (line: ConsoleLine): HTMLLIElement => {
    const item = node("li", "wi-line");
    item.dataset.level = line.level;
    item.dataset.source = line.source;
    item.dataset.run = String(line.run);
    const time = node("time", "wi-time", clock(line.at));
    time.dateTime = new Date(line.at).toISOString();
    const level = node(
      "span",
      "wi-level",
      line.source === "error"
        ? "uncaught"
        : line.source === "rejection"
          ? "rejection"
          : line.level,
    );
    item.append(
      node("span", "wi-run", `run ${line.run}`),
      time,
      level,
      node("span", "wi-text", line.text),
    );
    return item;
  };

  const drawDetail = (worker: WorkerSnapshot | null) => {
    detail.hidden = worker === null;
    pick.hidden = worker !== null || rows.size === 0;
    if (worker === null) {
      // Nothing of a worker that left stays in the page, shown or not.
      lines.replaceChildren();
      drawnLines.clear();
      drawnFor = null;
      factList.replaceChildren();
      problem.textContent = "";
      return;
    }
    name.textContent = worker.name;
    badge.textContent = stateWord(worker);
    badge.dataset.state = worker.state?.state ?? "stopped";
    problem.hidden = worker.state?.state !== "failed";
    problem.textContent =
      worker.state?.state === "failed" ? worker.state.reason : "";
    const rowsNow = facts(worker, Date.now());
    factList.replaceChildren(
      ...rowsNow.flatMap(([label, value]) => [
        node("dt", undefined, label),
        node("dd", undefined, value),
      ]),
    );
    restart.hidden = !worker.restartable;
    traces.disabled = worker.state?.state !== "running";
    drawLines(worker.name);
  };

  const render = () => {
    scheduled = false;
    const snapshot = workers.inspect();
    if (selected !== null && !snapshot.some((w) => w.name === selected)) {
      selected = null;
      note.textContent = "";
    }
    if (selected === null && snapshot.length > 0) selected = snapshot[0].name;
    drawList(snapshot);
    drawDetail(snapshot.find((worker) => worker.name === selected) ?? null);
  };

  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(render);
  };

  function select(workerName: string) {
    if (selected !== workerName) note.textContent = "";
    selected = workerName;
    render();
  }

  workers.onChange(() => {
    // A worker that left the session takes its lines with it.
    log.keepOnly(new Set(workers.inspect().map((worker) => worker.name)));
    schedule();
  });
  log.onChange(schedule);
  for (const [value, chip] of filterButtons)
    chip.addEventListener("click", () => {
      filter = value;
      for (const [other, otherChip] of filterButtons)
        otherChip.setAttribute("aria-pressed", String(other === value));
      render();
    });
  traces.addEventListener("click", () => {
    if (selected === null) return;
    deps.showTraces(selected);
  });
  restart.addEventListener("click", () => {
    const worker = current();
    if (worker === null || !worker.restartable) return;
    note.textContent = `Restarting ${worker.name}…`;
    void workers.restart(worker.name).then(() => {
      if (selected === worker.name) note.textContent = "";
    });
  });
  copy.addEventListener("click", () => {
    if (selected === null) return;
    const text = log
      .lines(selected)
      .filter((line) => passes(line, filter))
      .map(lineText)
      .join("\n");
    void copyInPage(text).then((copied) => {
      copy.textContent = copied ? "Copied" : "Copy failed";
      setTimeout(() => (copy.textContent = "Copy"), 1200);
    });
  });
  clear.addEventListener("click", () => {
    if (selected !== null) log.clear(selected);
  });

  render();
  return {
    show(next) {
      root.hidden = !next;
      clearInterval(ticker);
      if (next) {
        render();
        // The uptime line counts on while shown.
        ticker = setInterval(() => {
          const worker = current();
          if (worker !== null)
            factList.replaceChildren(
              ...facts(worker, Date.now()).flatMap(([label, value]) => [
                node("dt", undefined, label),
                node("dd", undefined, value),
              ]),
            );
        }, 1000);
      }
    },
    select,
  };
}
