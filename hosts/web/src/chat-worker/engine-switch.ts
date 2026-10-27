import type { WorkerState } from "../product-workers.js";
import type { ChatEngineMode, IdentityLink } from "./worker-mode.js";

/**
 * DEV ONLY. The Developer menu's "Chat engine" control: run the wallet's
 * Nova chat in the host (the default) or in the Nova chat worker, with a
 * short line on how the worker and its identity link are doing.
 */

export interface EngineSwitchDeps {
  section: HTMLElement;
  signedIn(): boolean;
  mode(): ChatEngineMode;
  setMode(mode: ChatEngineMode, options: { copyHistory: boolean }): void;
  link(): IdentityLink;
  worker(): WorkerState | null;
  productId: string;
}

function describe(deps: EngineSwitchDeps): string {
  if (!deps.signedIn()) return "Sign in to choose where chat runs.";
  if (deps.mode() === "host") return "The host runs chat for this wallet.";
  const worker = deps.worker();
  const link = deps.link();
  const workerLine =
    worker === null
      ? "Worker not started."
      : worker.state === "failed"
        ? `Worker failed: ${worker.reason}`
        : worker.state === "starting"
          ? "Worker starting…"
          : `Worker running as ${deps.productId}.`;
  const linkLine =
    link.state === "connected"
      ? `Identity lent: ${link.calls} operations${link.refused > 0 ? `, ${link.refused} refused` : ""}.`
      : link.state === "refused"
        ? `Identity refused: ${link.reason}`
        : link.state === "waiting"
          ? "Identity not asked for yet."
          : "";
  return `${workerLine} ${linkLine} The host's chat engine is off.`.trim();
}

export function drawEngineSwitch(deps: EngineSwitchDeps): {
  render(): void;
} {
  const select = document.createElement("select");
  select.id = "chat-engine";
  select.append(
    new Option("Host (default)", "host"),
    new Option("Nova chat worker", "worker"),
  );
  select.title =
    "Run this wallet's chat in the host, or in a product worker this dev server builds. Only one runs at a time.";
  const label = document.createElement("label");
  label.htmlFor = select.id;
  label.textContent = "Chat engine";

  const copy = document.createElement("input");
  copy.type = "checkbox";
  copy.id = "chat-engine-copy";
  const copyLabel = document.createElement("label");
  copyLabel.className = "check";
  copyLabel.htmlFor = copy.id;
  copyLabel.append(copy, " Copy the host's history into the worker");
  copyLabel.title =
    "Once, at this switch, and only into a worker with no history yet. The host keeps its copy; the two then diverge.";
  const copyRow = document.createElement("div");
  copyRow.className = "row";
  copyRow.append(copyLabel);

  const status = document.createElement("p");
  status.className = "status";
  status.dataset.chatEngine = "status";

  select.addEventListener("change", () => {
    try {
      deps.setMode(select.value === "worker" ? "worker" : "host", {
        copyHistory: copy.checked,
      });
    } catch (error) {
      status.textContent =
        error instanceof Error ? error.message : String(error);
    }
    copy.checked = false;
    render();
  });

  const render = () => {
    const signedIn = deps.signedIn();
    const mode = deps.mode();
    select.value = mode;
    select.disabled = !signedIn;
    copyRow.hidden = !signedIn || mode === "worker";
    status.textContent = describe(deps);
    status.dataset.mode = mode;
  };

  deps.section.append(label, select, copyRow, status);
  render();
  return { render };
}
