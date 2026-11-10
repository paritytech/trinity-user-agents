import type { RendererNode } from "@parity/truapi";
import {
  renderDiagnosisMarkdown,
  type DiagnosisResult,
} from "../shared/diagnosis";

/** Pinned against the generated service metadata by `chat-diagnosis.test.ts`. */
export const WORKER_DIAGNOSIS_METHODS = [
  "Chat/create_room",
  "Chat/register_bot",
  "Chat/list_subscribe",
  "Chat/post_message",
  "Chat/set_room_footer",
  "Chat/action_subscribe",
  "Renderer/render",
  "Renderer/action_subscribe",
] as const;

export type WorkerDiagnosisMethod = (typeof WORKER_DIAGNOSIS_METHODS)[number];

export const CHAT_DIAGNOSIS_REFRESH_ACTION = "truapi-chat-diagnosis-refresh";
export const CHAT_DIAGNOSIS_COPY_ACTION = "truapi-chat-diagnosis-copy";
export const CHAT_DIAGNOSIS_RENDERER_KICKER = "NATIVE CUSTOM MESSAGE";
export const CHAT_DIAGNOSIS_RENDERER_TITLE = "Custom message rendered ✓";
export const CHAT_DIAGNOSIS_RENDERER_DESCRIPTION =
  "This panel is a live native renderer tree from TrUAPI Playground.";

type CopyStatus = "idle" | "copied" | "unavailable";

const STATUS_ICON = {
  idle: "·",
  running: "↻",
  pass: "✓",
  fail: "✕",
  skipped: "–",
} as const;

export class ChatDiagnosis {
  readonly #results = new Map<WorkerDiagnosisMethod, DiagnosisResult>();
  readonly #onChange: () => void;
  #copyStatus: CopyStatus = "idle";

  constructor(onChange: () => void = () => {}) {
    this.#onChange = onChange;
    for (const id of WORKER_DIAGNOSIS_METHODS) {
      this.#results.set(id, { id, status: "running" });
    }
  }

  pass(id: WorkerDiagnosisMethod, details: string): void {
    if (this.#results.get(id)?.status === "fail") return;
    this.#results.set(id, { id, status: "pass", details });
    this.#onChange();
  }

  fail(id: WorkerDiagnosisMethod, error: unknown): void {
    this.#results.set(id, { id, status: "fail", details: errorDetails(error) });
    this.#onChange();
  }

  failPending(error: unknown): void {
    const details = errorDetails(error);
    for (const id of WORKER_DIAGNOSIS_METHODS) {
      if (this.#results.get(id)?.status === "running") {
        this.#results.set(id, { id, status: "fail", details });
      }
    }
    this.#onChange();
  }

  results(): DiagnosisResult[] {
    return WORKER_DIAGNOSIS_METHODS.map((id) => ({ ...this.#results.get(id)! }));
  }

  isComplete(): boolean {
    return this.results().every(
      ({ status }) => status === "pass" || status === "fail",
    );
  }

  markdown(): string {
    return renderDiagnosisMarkdown(this.results(), {
      title: "Truapi Chat Diagnosis",
    });
  }

  copied(): void {
    this.#copyStatus = "copied";
    this.#onChange();
  }

  copyUnavailable(): void {
    this.#copyStatus = "unavailable";
    this.#onChange();
  }

  rendererNode(): RendererNode {
    const results = this.results();
    const passed = results.filter(({ status }) => status === "pass").length;
    const failed = results.filter(({ status }) => status === "fail").length;
    return column([
      text(CHAT_DIAGNOSIS_RENDERER_KICKER, "BodySmallRegular"),
      text(CHAT_DIAGNOSIS_RENDERER_TITLE, "HeadlineLarge"),
      text(CHAT_DIAGNOSIS_RENDERER_DESCRIPTION, "BodySmallRegular"),
      text(
        `Chat diagnosis · ${passed} success · ${failed} failed`,
        "BodyMediumRegular",
      ),
      ...results.map((result) =>
        text(
          `${STATUS_ICON[result.status]} ${result.id.replace("Chat/", "")}`,
          "BodySmallRegular",
        ),
      ),
      button("Refresh results", CHAT_DIAGNOSIS_REFRESH_ACTION),
      button(
        this.#copyStatus === "copied" ? "Copied ✓" : "Copy report",
        CHAT_DIAGNOSIS_COPY_ACTION,
      ),
      ...(this.#copyStatus === "unavailable"
        ? [
            text(
              "Clipboard unavailable; long-press the report message below to copy it.",
              "BodySmallRegular",
            ),
          ]
        : []),
    ]);
  }
}

function errorDetails(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function column(children: RendererNode[]): RendererNode {
  return {
    tag: "Column",
    value: {
      modifiers: [],
      props: { horizontalAlignment: "Start", verticalArrangement: "Start" },
      children,
    },
  };
}

function text(
  value: string,
  style: "HeadlineLarge" | "BodyMediumRegular" | "BodySmallRegular",
): RendererNode {
  return {
    tag: "Text",
    value: {
      modifiers: [],
      props: { style },
      children: [{ tag: "String", value: { text: value } }],
    },
  };
}

function button(text: string, clickAction: string): RendererNode {
  return {
    tag: "Button",
    value: {
      modifiers: [],
      props: {
        text,
        variant: "Secondary",
        enabled: true,
        loading: false,
        clickAction,
      },
      children: [],
    },
  };
}
