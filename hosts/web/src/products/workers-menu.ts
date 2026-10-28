import type { WorkerState } from "./product-workers.js";

/**
 * The menu's Workers section: the product workers the signed-in wallet runs,
 * the form to add one by name, and Remove. A worker runs while the wallet is
 * signed in and this tab is open; its rooms show in Chat.
 */

export interface WorkerList {
  names(): string[];
  state(name: string): WorkerState | null;
  isDefault(name: string): boolean;
  add(name: string): Promise<void>;
  remove(name: string): void;
  onChange(listener: () => void): () => void;
}

export interface WorkersMenuDeps {
  form: HTMLFormElement;
  input: HTMLInputElement;
  add: HTMLButtonElement;
  list: HTMLUListElement;
  status: HTMLElement;
  workers: WorkerList;
  signedIn(): boolean;
  /** Calls back when the wallet session changes. */
  onSessionChange(listener: () => void): void;
}

function stateLine(state: WorkerState | null): string {
  if (state === null) return "Stopped";
  if (state.state === "running")
    return `Running · v${state.manifest.appVersion.join(".")}`;
  if (state.state === "starting") return "Starting…";
  return `Failed: ${state.reason}`;
}

export function bindWorkersMenu(deps: WorkersMenuDeps): void {
  const { form, input, add, list, status, workers } = deps;
  let busy = false;
  let problem: string | null = null;

  const render = (): void => {
    const signedIn = deps.signedIn();
    input.disabled = !signedIn;
    add.disabled = !signedIn || busy;
    const rows = workers.names().map((name) => {
      const state = workers.state(name);
      const item = document.createElement("li");
      item.dataset.worker = name;
      item.dataset.state = state?.state ?? "stopped";
      const title = document.createElement("span");
      title.className = "worker-name";
      title.textContent = name;
      const line = document.createElement("span");
      line.className = "worker-state";
      line.textContent = workers.isDefault(name)
        ? `${stateLine(state)} · added by this host`
        : stateLine(state);
      item.append(title, line);
      if (!workers.isDefault(name)) {
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "quiet";
        remove.textContent = "Remove";
        remove.setAttribute("aria-label", `Remove ${name}`);
        remove.addEventListener("click", () => workers.remove(name));
        item.append(remove);
      }
      return item;
    });
    list.replaceChildren(...rows);
    list.hidden = rows.length === 0;
    status.dataset.state = problem === null ? "" : "error";
    status.textContent =
      problem ??
      (signedIn
        ? rows.length === 0
          ? "No workers."
          : ""
        : "Sign in with a wallet to run workers.");
    status.hidden = status.textContent === "";
  };

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    busy = true;
    problem = null;
    render();
    workers
      .add(input.value)
      .then(() => (input.value = ""))
      .catch(
        (error: unknown) =>
          (problem = error instanceof Error ? error.message : String(error)),
      )
      .finally(() => {
        busy = false;
        render();
      });
  });
  input.addEventListener("input", () => {
    if (problem === null) return;
    problem = null;
    render();
  });
  workers.onChange(render);
  deps.onSessionChange(() => {
    // A name typed for one wallet is not added to the next.
    input.value = "";
    problem = null;
    render();
  });
  render();
}
