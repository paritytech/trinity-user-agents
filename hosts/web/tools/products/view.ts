import "../statements/statements-view.css";
import "./products-view.css";
import type { CarSkip } from "../../src/products/archive/gateway.js";
import {
  type ContentSource,
  type ProductLoadSummary,
  type Transport,
  transportOf,
} from "../../src/products/load-observer.js";
import {
  MAX_LOADS,
  type ProductLoadLog,
  type ProductLoadRecord,
} from "./log.js";
import {
  button,
  clock,
  el,
  fact,
  more,
  openDisclosures,
  reopenDisclosures,
} from "../statements/ui.js";

/**
 * The inspector's Products view: one row per mounted load, with what the
 * host resolved for it and what its loader counted (`load-observer.ts`).
 * Nothing here is worked out from log lines.
 */

export interface ProductsView {
  show(visible: boolean): void;
}

/** How often the view draws at most while shown. */
const DRAW_MS = 250;

const COLUMNS = [
  "Product",
  "Size",
  "Files",
  "Arrived",
  "Load",
  "Transport",
  "Outcome",
] as const;

const TRANSPORT_LABEL: Record<Transport, string> = {
  car: "CAR",
  "car+raw": "CAR+raw",
  raw: "raw",
  cache: "cache",
  none: "none",
};

const ABOUT = [
  "Host facts (record, CID, gateway, resolve) come from the host page when it opens the mount. Loader facts come from the host's loader page in the product's frame, sent once per load before the product starts.",
  "The product then runs on this origin and could post a summary too. The host keeps the first one per frame, which is the loader's.",
  "Size is the verified files' bytes, not bytes on the wire. Wire bytes are under CAR stream and Block by block.",
  "Arrived counts the blocks that arrived, by the CAR stream and block by block together. After the hedge a block can arrive both ways and count twice.",
  "Cache: the verified copy in Cache Storage was used and nothing was fetched. Its file counts were written when the copy was stored; a copy stored by an earlier build has none.",
  "A CAR stream can still run when the load finishes; its counts are as of the summary.",
  "Resolve is the DotNS lookup and the check of what the content is. For Contact Chat it is reading the host's own description of the archive it serves.",
  "Served by this host, and seed (host origin): the blocks are read from this host's origin, not a gateway (a shipped seed or the Contact Chat worker), and each is checked against its CID. The loader asks the host origin for no CAR stream, since a static host has none to send: such a load reads block by block, and its CAR stream reads not tried (host origin).",
  `In memory for this session, at most ${MAX_LOADS} loads, newest first. Opening the same mount again replaces its row.`,
];

/** `count` bytes, in the unit that reads best. */
export function size(count: number): string {
  if (count < 1024) return `${count} B`;
  if (count < 1024 * 1024) return `${(count / 1024).toFixed(1)} KiB`;
  return `${(count / 1024 / 1024).toFixed(2)} MiB`;
}

/** `ms` milliseconds, as ms under a second and as seconds above. */
export function duration(ms: number): string {
  return ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(2)} s`;
}

/** Blocks that arrived by either transport. A block that came both ways counts twice. */
function blocksArrived(summary: ProductLoadSummary): number {
  return (summary.car?.blocks ?? 0) + (summary.raw?.blocks ?? 0);
}

/** How a row names where its blocks were read, when not from the gateway. */
const SOURCE_BADGE: Record<ContentSource, string | null> = {
  gateway: null,
  host: "served by this host",
  seed: "seed (host origin)",
};

/** A load's transport, with where its blocks came from when not the gateway. */
export function transportText(
  transport: Transport,
  source: ContentSource,
): string {
  const label = TRANSPORT_LABEL[transport];
  return source === "gateway" || transport === "cache" || transport === "none"
    ? label
    : `${label} · ${source === "seed" ? "seed" : "host"}`;
}

/**
 * Where a load stands before its loader reports: waiting, or closed first,
 * or `failed` when its frame never opened.
 */
function pendingOutcome(record: ProductLoadRecord): string {
  if (record.closeReason !== undefined) return "failed";
  return record.closedAt === undefined ? "waiting" : "closed";
}

/** A load's row values, in {@link COLUMNS} order after the product. */
function rowValues(record: ProductLoadRecord): string[] {
  const { summary } = record;
  if (summary === undefined) {
    const pending = record.closedAt === undefined ? "…" : "–";
    return [
      pending,
      pending,
      pending,
      pending,
      pending,
      pendingOutcome(record),
    ];
  }
  const files = summary.files;
  return [
    files === undefined ? "–" : size(files.bytes),
    files === undefined ? "–" : String(files.count),
    summary.outcome === "cached" ? "–" : String(blocksArrived(summary)),
    duration(summary.totalMs),
    transportText(transportOf(summary), record.source),
    summary.outcome,
  ];
}

export function mountProductsView(
  root: HTMLElement,
  log: ProductLoadLog,
): ProductsView {
  const top = el("div", "si-top");
  const title = el("span", "si-title", "Products ");
  title.append(el("span", "accent", "Inspector"));
  const summaryLine = el("span", "si-summary");
  top.append(title, summaryLine);
  const about = el("details", "si-caveats");
  const aboutList = el("ul", "si-note si-about-list");
  aboutList.append(...ABOUT.map((line) => el("li", undefined, line)));
  about.append(el("summary", undefined, "About this data"), aboutList);
  const body = el("div", "si-body");
  root.append(top, about, body);

  let visible = false;
  let dirty = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let selected: string | null = null;

  function detail(record: ProductLoadRecord): HTMLElement {
    const section = el("section", "si-detail");
    section.dataset.key = `load:${record.mount}`;
    section.setAttribute("aria-label", `Load of ${record.product}`);
    const host = el("dl", "si-facts");
    host.append(
      ...fact(
        "Record",
        record.record,
        "host",
        "Where the content came from. For a name, the DotNS record that answered: app.<name> is tried first, then <name>. Contact Chat's is the host's own description of the archive it serves.",
      ),
      ...fact("Content", record.cid, "host"),
      ...fact(
        "Kind",
        record.kind === "car" ? "archive (one CAR file)" : "site (a directory)",
        "host",
      ),
      ...fact(
        "Gateway",
        record.source === "gateway"
          ? record.gateway
          : `this host (${record.gateway})`,
        "host",
        "Where the loader fetches the blocks. Each block is checked against its CID wherever it comes from.",
      ),
      ...(record.resolveMs === undefined
        ? []
        : fact(
            "Resolve",
            duration(record.resolveMs),
            "host",
            "From the start of the lookup to a content id and kind, before the frame opened.",
          )),
      ...fact("Execution", record.execution, "host"),
      ...fact("Mount", record.mount, "host"),
      ...fact("Opened", clock(record.openedAt), "host"),
      ...(record.closedAt === undefined
        ? []
        : fact(
            "Closed",
            record.closeReason === undefined
              ? clock(record.closedAt)
              : `${clock(record.closedAt)} · the frame never opened: ${record.closeReason}`,
            "host",
          )),
    );
    section.append(el("h4", "si-sub", "Where from"), host);
    const { summary } = record;
    if (summary === undefined) {
      section.append(
        el(
          "p",
          "si-empty",
          record.closedAt === undefined
            ? "No summary from the loader yet. It sends one when the load ends."
            : "No summary: the frame closed before its loader reported.",
        ),
      );
      return section;
    }
    section.append(el("h4", "si-sub", "Load"), loadFacts(record, summary));
    if (summary.car?.requested)
      section.append(el("h4", "si-sub", "CAR stream"), carFacts(summary));
    else if (summary.carSkipped !== undefined)
      section.append(
        el("h4", "si-sub", "CAR stream"),
        skippedFacts(summary.carSkipped),
      );
    if (summary.raw !== undefined && summary.raw.requests > 0)
      section.append(el("h4", "si-sub", "Block by block"), rawFacts(summary));
    return section;
  }

  function loadFacts(
    record: ProductLoadRecord,
    summary: ProductLoadSummary,
  ): HTMLElement {
    const facts = el("dl", "si-facts");
    const transport = transportOf(summary);
    facts.append(
      ...fact(
        "Outcome",
        summary.reason === undefined
          ? summary.outcome
          : `${summary.outcome}: ${summary.reason}`,
        "loader",
      ),
      ...fact(
        "Transport",
        transportText(transport, record.source),
        "derived",
        "CAR: every read came from the CAR stream. CAR+raw: the rest were read block by block. raw: block by block only. cache: nothing fetched. · host or · seed: the blocks came from this host's origin, not the gateway.",
      ),
      ...fact(
        "Total",
        duration(summary.totalMs),
        "loader",
        "From the loader's start until it hands the frame to the product, or gives up.",
      ),
    );
    const phases: string[] = [];
    if (summary.workerMs !== undefined)
      phases.push(`service worker ${duration(summary.workerMs)}`);
    if (summary.fetchMs !== undefined && summary.loadMs !== undefined)
      phases.push(
        `fetch ${duration(summary.fetchMs)}`,
        `verify and unpack ${duration(Math.max(0, summary.loadMs - summary.fetchMs))}`,
      );
    else if (summary.waitMs !== undefined && summary.loadMs !== undefined)
      phases.push(
        `fetch and unpack ${duration(summary.loadMs)} (waiting on blocks ${duration(summary.waitMs)})`,
      );
    if (summary.storeMs !== undefined)
      phases.push(`store ${duration(summary.storeMs)}`);
    if (phases.length > 0)
      facts.append(
        ...fact(
          "Phases",
          phases.join(" · "),
          "loader",
          record.kind === "car"
            ? "Fetch runs until the last block read was answered: every block of the archive file is read before the file is parsed. Verify and unpack is the rest of the load: parsing the archive, checking its blocks and unpacking its files. A CAR stream may still be bringing blocks no read asked for. Store writes the files to Cache Storage."
            : "A site's blocks are fetched while it is unpacked, so the two are not split. Waiting on blocks is the time at least one read was open.",
        ),
      );
    const files = summary.files;
    if (files !== undefined) {
      facts.append(
        ...fact(
          "Files",
          `${files.count} · ${size(files.bytes)} (${files.bytes} B)`,
          "loader",
          summary.outcome === "cached"
            ? "Counted when the copy was stored."
            : "The verified files, as unpacked.",
        ),
      );
      if (files.largest !== undefined)
        facts.append(
          ...fact(
            "Largest",
            `${files.largest.path} · ${size(files.largest.bytes)}`,
            "loader",
          ),
        );
    } else if (summary.outcome === "cached")
      facts.append(
        ...fact(
          "Files",
          "not known",
          "loader",
          "The copy was stored by an earlier build, which kept no counts.",
        ),
      );
    if (summary.reads !== undefined)
      facts.append(
        ...fact(
          "Reads",
          String(summary.reads),
          "loader",
          "Block reads the unpacking made, a block linked twice counted twice.",
        ),
      );
    return facts;
  }

  function carFacts(summary: ProductLoadSummary): HTMLElement {
    const car = summary.car!;
    const facts = el("dl", "si-facts");
    facts.append(
      ...fact(
        "Ended",
        car.end === undefined
          ? "still running when the load ended"
          : car.reason === undefined
            ? car.end
            : `${car.end}: ${car.reason}`,
        "loader",
        "complete: the last byte arrived. slow: under the minimum rate in a window. refused: not a 2xx CAR. failed: the request or the stream broke. cancelled: the load stopped it.",
      ),
      ...(car.status === undefined
        ? []
        : fact("Status", String(car.status), "loader")),
      ...(car.firstByteMs === undefined
        ? []
        : fact("First byte", duration(car.firstByteMs), "loader")),
      ...(car.ms === undefined ? [] : fact("Ran", duration(car.ms), "loader")),
      ...fact("Arrived", `${car.blocks} blocks · ${size(car.bytes)}`, "loader"),
      ...fact(
        "Hedge",
        car.hedged ? "fired" : "not fired",
        "loader",
        "Fired: the hedge time passed while the stream ran, so reads stopped waiting for it and read missing blocks one by one.",
      ),
      ...fact(
        "Reads",
        `${car.fromCar} from the stream · ${car.fallback} block by block`,
        "loader",
      ),
    );
    return facts;
  }

  function skippedFacts(skip: CarSkip): HTMLElement {
    const facts = el("dl", "si-facts");
    facts.append(
      ...fact(
        "Ended",
        skip === "host-origin" ? "not tried (host origin)" : "not tried (off)",
        "loader",
        skip === "host-origin"
          ? "The blocks come from this host's own origin, a static server with no CAR to send, so the loader read them block by block."
          : "FETCH_CAR is off in this build, so the loader read the blocks block by block.",
      ),
    );
    return facts;
  }

  function rawFacts(summary: ProductLoadSummary): HTMLElement {
    const raw = summary.raw!;
    const facts = el("dl", "si-facts");
    facts.append(
      ...fact("Requests", `${raw.requests} (${raw.retries} retried)`, "loader"),
      ...fact("Arrived", `${raw.blocks} blocks · ${size(raw.bytes)}`, "loader"),
      ...fact(
        "Reused",
        `${raw.cacheHits} from memory · ${raw.joined} joined an open request`,
        "loader",
      ),
    );
    return facts;
  }

  function row(record: ProductLoadRecord): HTMLElement[] {
    const node = button("pi-row", "", `Show the load of ${record.product}`);
    node.setAttribute("aria-pressed", String(record.mount === selected));
    node.dataset.product = record.product;
    node.dataset.outcome = record.summary?.outcome ?? pendingOutcome(record);
    node.dataset.focusKey = `pi-row:${record.mount}`;
    const name = el("span", "si-cell pi-name", record.product);
    name.append(" ", el("span", "si-badge", record.execution));
    const badge = SOURCE_BADGE[record.source];
    if (badge !== null) name.append(" ", el("span", "si-badge", badge));
    node.append(name);
    rowValues(record).forEach((value, index) => {
      const cell = el("span", "si-cell", value);
      cell.dataset.label = COLUMNS[index + 1];
      node.append(cell);
    });
    node.addEventListener("click", () => {
      selected = record.mount === selected ? null : record.mount;
      now();
    });
    return record.mount === selected ? [node, detail(record)] : [node];
  }

  const draw = () => {
    timer = undefined;
    if (!visible) return;
    dirty = false;
    const loads = log.list();
    const total = loads.reduce(
      (sum, record) => sum + (record.summary?.files?.bytes ?? 0),
      0,
    );
    const dropped = log.dropped();
    summaryLine.textContent = `${loads.length} load${loads.length === 1 ? "" : "s"} · ${size(total)} of files${dropped > 0 ? ` · ${dropped} older dropped` : ""}`;
    const opened = openDisclosures(body);
    // Rows are drawn again each time: the focused one gets its focus back.
    const active = document.activeElement as HTMLElement | null;
    const focusKey =
      active !== null && body.contains(active)
        ? active.dataset.focusKey
        : undefined;
    if (loads.length === 0) {
      const empty = el("div", "si-empty");
      empty.append(
        "No product loaded by name yet. ",
        more(
          "A product opened by its DotNS name, a Pocket card's widget, and each product worker (Contact Chat's too) show here once their frame starts loading. A product opened by URL has no loader.",
        ),
      );
      body.replaceChildren(empty);
    } else {
      const table = el("div", "pi-list");
      table.setAttribute("role", "list");
      const head = el("div", "pi-head");
      head.setAttribute("aria-hidden", "true");
      for (const name of COLUMNS) head.append(el("span", undefined, name));
      table.append(head, ...loads.flatMap(row));
      body.replaceChildren(table);
    }
    reopenDisclosures(body, opened);
    if (focusKey !== undefined)
      [...body.querySelectorAll<HTMLElement>("[data-focus-key]")]
        .find((node) => node.dataset.focusKey === focusKey)
        ?.focus();
  };

  const now = () => {
    clearTimeout(timer);
    timer = undefined;
    dirty = true;
    draw();
  };

  log.onChange(() => {
    dirty = true;
    if (visible && timer === undefined) timer = setTimeout(draw, DRAW_MS);
  });

  return {
    show(next) {
      visible = next;
      root.hidden = !next;
      if (next && dirty) now();
    },
  };
}
