import { createInAppDebugger } from "@parity/truapi-debugger";
import type { FrameTap } from "../src/products/tap.js";
import type { ActivityLog } from "./activity/log.js";
import { mountActivityLog } from "./activity/view.js";
import { bindDockResize } from "./dock-resize.js";
import { mountChatDiagnostics } from "./inspector/chat-diagnostics-view.js";
import type { ChatDiagnostics } from "./inspector/diagnostics.js";
import { createFrameTap } from "./inspector/frame-tap.js";
import { bindViewport } from "./viewport.js";

/** What the developer tools need from the page. */
export interface ToolsPage {
  element<T extends HTMLElement>(id: string): T;
  /** The product frame the Preview tab sizes. */
  productFrame: HTMLElement;
  /** What the host did, for the inspector's Activity view. */
  activity: ActivityLog;
  /** The session's chat events and payloads, for the inspector's Chat view. */
  diagnostics: ChatDiagnostics;
}

/** The developer tools mounted in the page. */
export interface Tools {
  /** Feed one product's or worker's relayed frames to the inspector. */
  tap(productId: string, coreWireSchemaHash: string | undefined): FrameTap;
}

const INSPECTOR_NOTES = {
  activity:
    "What the host did, by category. Kept in memory in this tab, at most the last 500 events.",
  wire: "Frames stay in this tab and may carry sensitive payloads.",
  chat: "Chat events are not TrUAPI frames. Metadata only unless payload capture is on; kept in memory and cleared when the session changes.",
} as const;

type InspectorView = keyof typeof INSPECTOR_NOTES;

/**
 * Mount the developer tools' inspector, with its Activity, TrUAPI and Chat
 * views, the dock's resize handle and the Preview tab's sizes.
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
  const chatDiagnosticsView = mountChatDiagnostics(
    element("chat-diagnostics"),
    page.diagnostics,
  );
  const inspectorNote = element("inspector-note");
  const inspectorViewButtons = [
    ...document.querySelectorAll<HTMLButtonElement>("[data-inspector-view]"),
  ];
  /** Show one of the inspector's views. */
  function showInspectorView(view: InspectorView): void {
    for (const button of inspectorViewButtons) {
      const selected = viewOf(button) === view;
      button.setAttribute("aria-selected", String(selected));
      button.tabIndex = selected ? 0 : -1;
    }
    activityView.show(view === "activity");
    inspectorMount.hidden = view !== "wire";
    chatDiagnosticsView.show(view === "chat");
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
  };
}
