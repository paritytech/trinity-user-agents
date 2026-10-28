import { createInAppDebugger } from "@parity/truapi-debugger";
import type { FrameTap } from "../src/products/tap.js";
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
  /** The session's chat events and payloads, for the inspector's Chat view. */
  diagnostics: ChatDiagnostics;
}

/** The developer tools mounted in the page. */
export interface Tools {
  /** Feed one product's or worker's relayed frames to the inspector. */
  tap(productId: string, coreWireSchemaHash: string | undefined): FrameTap;
}

const INSPECTOR_NOTES = {
  wire: "Frames stay in this tab and may carry sensitive payloads.",
  chat: "Chat events are not TrUAPI frames. Metadata only unless payload capture is on; kept in memory and cleared when the session changes.",
} as const;

/**
 * Mount the developer tools' inspector, with its TrUAPI traces and its Chat
 * view, the dock's resize handle and the Preview tab's sizes.
 */
export function mountTools(page: ToolsPage): Tools {
  const { element } = page;
  const inspector = createInAppDebugger();
  const inspectorMount = element("inspector-mount");
  inspector.mount(inspectorMount);
  const chatDiagnosticsView = mountChatDiagnostics(
    element("chat-diagnostics"),
    page.diagnostics,
  );
  const inspectorNote = element("inspector-note");
  const inspectorViewButtons = [
    ...document.querySelectorAll<HTMLButtonElement>("[data-inspector-view]"),
  ];
  /** Show the TrUAPI traces or the chat events in the inspector. */
  function showInspectorView(view: keyof typeof INSPECTOR_NOTES): void {
    for (const button of inspectorViewButtons)
      button.setAttribute(
        "aria-selected",
        String(button.dataset.inspectorView === view),
      );
    inspectorMount.hidden = view !== "wire";
    chatDiagnosticsView.show(view === "chat");
    inspectorNote.textContent = INSPECTOR_NOTES[view];
  }
  for (const button of inspectorViewButtons)
    button.addEventListener("click", () =>
      showInspectorView(
        button.dataset.inspectorView === "chat" ? "chat" : "wire",
      ),
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
