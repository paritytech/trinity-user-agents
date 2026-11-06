import { createInAppDebugger } from "@parity/truapi-debugger";
import type { FrameTap } from "../src/products/tap.js";
import { workerChannel } from "../src/products/product-workers.js";
import type { ActivityLog } from "./activity/log.js";
import { mountActivityLog } from "./activity/view.js";
import { bindDockResize } from "./dock-resize.js";
import { createFrameTap } from "./inspector/frame-tap.js";
import type { PeopleAccess } from "./statements/actions.js";
import type { StatementLedger } from "./statements/ledger.js";
import { mountStatementsView } from "./statements/view.js";
import { mountChatView } from "./chat/view.js";
import type { ProductLoadLog } from "./products/log.js";
import { mountProductsView } from "./products/view.js";
import { bindViewport } from "./viewport.js";
import type { WorkerConsoleLog } from "./workers/console-log.js";
import { mountWorkersView, type WorkerSource } from "./workers/view.js";

/** What the developer tools need from the page. */
export interface ToolsPage {
  element<T extends HTMLElement>(id: string): T;
  /** The product frame the Preview tab sizes. */
  productFrame: HTMLElement;
  /** What the host did, for the inspector's Activity view. */
  activity: ActivityLog;
  /** The product workers and their console lines, for the inspector's Workers view. */
  workers: WorkerSource;
  workerConsole: WorkerConsoleLog;
  /** What the Statements view draws. */
  statements: StatementLedger;
  /** The People chain, for the Statements view's reads on demand, or null before the network is up. */
  people(): PeopleAccess | null;
  /** The network's id, for the Statements view's evidence. */
  network(): string;
  /** What the Products view draws. */
  productLoads: ProductLoadLog;
}

/** The developer tools mounted in the page. */
export interface Tools {
  /** Feed one product's or worker's relayed frames to the inspector. */
  tap(productId: string, coreWireSchemaHash: string | undefined): FrameTap;
  /** Show `name`'s worker in the inspector's Workers view. */
  inspectWorker(name: string): void;
  /** The signed-in session is `key` now: the Statements and Products views start again for a new one. */
  statementsSession(key: string, label: string): void;
}

const INSPECTOR_NOTES = {
  activity:
    "What the host did, by category. Kept in memory in this tab, at most the last 500 events.",
  wire: "Frames stay in this tab and may carry sensitive payloads.",
  workers:
    "Product workers' state, and their console lines: the workers' own text, kept in memory and cleared when the session changes.",
  statements:
    "Statement Store traffic, claims and grants, in memory for this session.",
  chat: "Conversations product workers report about themselves.",
  products:
    "What the loader fetched, checked and stored for each product it loaded, in memory for this session.",
} as const;

type InspectorView = keyof typeof INSPECTOR_NOTES;

/**
 * Mount the developer tools' inspector, with its Activity, TrUAPI, Workers,
 * Statements, Chat and Products views, the dock's resize handle and the Preview tab's
 * sizes.
 */
export function mountTools(page: ToolsPage): Tools {
  const { element } = page;
  const inspector = createInAppDebugger();
  const inspectorMount = element("inspector-mount");
  inspector.mount(inspectorMount);
  const activityView = mountActivityLog(
    element("activity-inspector"),
    element<HTMLOListElement>("log"),
    page.activity,
  );
  const inspectorNote = element("inspector-note");
  const inspectorViewButtons = [
    ...document.querySelectorAll<HTMLButtonElement>("[data-inspector-view]"),
  ];
  /** Show the TrUAPI view narrowed to `name`'s worker, or say why it cannot be. */
  function showWorkerTraces(name: string): void {
    showInspectorView("wire");
    const channel = workerChannel(name);
    const channels = new Set(
      inspector.session.traceEngine.traces().map((trace) => trace.channelId),
    );
    if (!channels.has(channel)) {
      inspectorNote.textContent = `No TrUAPI frames from ${name}'s worker yet. They show here once it calls the core.`;
      return;
    }
    // The TrUAPI view lists one channel per product and worker, and narrows
    // to one through its own chips, drawn once a second channel is seen.
    if (channels.size === 1) return;
    const chip = inspectorMount.querySelector<HTMLButtonElement>(
      `.ins-chan[data-channel="${CSS.escape(channel)}"]`,
    );
    if (chip === null)
      inspectorNote.textContent = `Choose ${channel} in the channel list to see only its frames.`;
    else chip.click();
  }
  const workersView = mountWorkersView(element("workers-inspector"), {
    workers: page.workers,
    log: page.workerConsole,
    showTraces: showWorkerTraces,
  });
  const statementsView = mountStatementsView(element("statements-inspector"), {
    ledger: page.statements,
    people: page.people,
    network: page.network,
    showTraces: showWorkerTraces,
    inspectWorker(name) {
      showInspectorView("workers");
      workersView.select(name);
    },
    showChat(key) {
      showInspectorView("chat");
      chatView.reveal(key);
    },
  });
  const chatView = mountChatView(element("chat-inspector"), {
    ledger: page.statements,
    showStatements(target) {
      showInspectorView("statements");
      statementsView.reveal(target);
    },
    showTraces: showWorkerTraces,
    inspectWorker(name) {
      showInspectorView("workers");
      workersView.select(name);
    },
  });
  const productsView = mountProductsView(
    element("products-inspector"),
    page.productLoads,
  );
  /** Show one of the inspector's views. */
  function showInspectorView(view: InspectorView): void {
    for (const button of inspectorViewButtons) {
      const selected = viewOf(button) === view;
      button.setAttribute("aria-selected", String(selected));
      button.tabIndex = selected ? 0 : -1;
    }
    activityView.show(view === "activity");
    inspectorMount.hidden = view !== "wire";
    workersView.show(view === "workers");
    statementsView.show(view === "statements");
    chatView.show(view === "chat");
    productsView.show(view === "products");
    inspectorNote.textContent = INSPECTOR_NOTES[view];
  }
  function viewOf(button: HTMLButtonElement): InspectorView {
    const view = button.dataset.inspectorView;
    return view !== undefined && view in INSPECTOR_NOTES
      ? (view as InspectorView)
      : "wire";
  }
  for (const button of inspectorViewButtons)
    button.addEventListener("click", () => showInspectorView(viewOf(button)));
  // Arrow keys move between the views, as between any tabs.
  inspectorViewButtons[0]?.parentElement?.addEventListener(
    "keydown",
    (event) => {
      const step =
        event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : null;
      if (step === null) return;
      event.preventDefault();
      const at = inspectorViewButtons.findIndex(
        (button) => button.getAttribute("aria-selected") === "true",
      );
      const next =
        inspectorViewButtons[
          (at + step + inspectorViewButtons.length) %
            inspectorViewButtons.length
        ];
      showInspectorView(viewOf(next));
      next.focus();
    },
  );
  bindDockResize(element("devtools-resize"), element("devtools"));

  bindViewport({
    select: element<HTMLSelectElement>("viewport"),
    scale: element("viewport-scale"),
    area: element("viewport-area"),
    frame: page.productFrame,
    fills: window.matchMedia("(max-width: 900px)"),
    custom: {
      group: element("viewport-custom"),
      width: element<HTMLInputElement>("viewport-width"),
      height: element<HTMLInputElement>("viewport-height"),
    },
    handle: element("viewport-handle"),
    note: element("viewport-note"),
    caption: element("viewport-caption"),
    presets: [
      ...document.querySelectorAll<HTMLButtonElement>(
        "#dev-preview [data-viewport]",
      ),
    ],
    frames: [
      ...document.querySelectorAll<HTMLButtonElement>(
        "#dev-preview [data-frame]",
      ),
    ],
    frameGroup: element("viewport-frames"),
  });

  return {
    tap: (productId, coreWireSchemaHash) =>
      createFrameTap(inspector, productId, coreWireSchemaHash),
    inspectWorker(name) {
      showInspectorView("workers");
      workersView.select(name);
    },
    statementsSession(key, label) {
      if (page.statements.startSession(key, label))
        statementsView.sessionChanged();
      page.productLoads.startSession(key);
    },
  };
}
