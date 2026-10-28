/**
 * DEV ONLY. The Developer menu's "Chat demo" control: start, stop or restart
 * the synthetic chat product (`tools/demos/local-demo.ts`) as one of the
 * signed-in wallet's workers. Its rooms show in Chat; what it is stays in a
 * folded note.
 */

export interface ChatDemoControl {
  productId: string;
  /** Where its code comes from. */
  source: string;
  running(): boolean;
  start(): void;
  stop(): void;
  restart(): void;
}

export interface ChatDemoDeps {
  section: HTMLElement;
  demo: ChatDemoControl;
  signedIn(): boolean;
}

export function drawChatDemo(deps: ChatDemoDeps): { render(): void } {
  const { demo } = deps;
  const block = document.createElement("div");
  block.id = "chat-demo";
  const label = document.createElement("label");
  label.textContent = "Chat demo";
  const status = document.createElement("p");
  status.className = "status";

  const action = (text: string, name: string, run: () => void) => {
    const node = document.createElement("button");
    node.type = "button";
    node.textContent = text;
    node.dataset.action = name;
    node.addEventListener("click", () => {
      run();
      render();
    });
    return node;
  };
  const start = action("Start", "demo-start", () => demo.start());
  const stop = action("Stop", "demo-stop", () => demo.stop());
  const restart = action("Restart", "demo-restart", () => demo.restart());
  const actions = document.createElement("div");
  actions.className = "row";
  actions.append(start, stop, restart);

  const about = document.createElement("details");
  about.className = "fold";
  const summary = document.createElement("summary");
  summary.textContent = "About the chat demo";
  const text = document.createElement("p");
  text.className = "status";
  text.textContent = `A made-up chat product run as a product worker, through the same loader, archive check and core as any worker. Its rooms (Ada, a group with Ada and Grace, and Lin) live in memory and nothing leaves this tab. It speaks the Chat Extension v2 draft bridge, not TrUAPI, and is not the Verbosity chat product. Its calls use a synthetic engine that opens no camera or microphone. Product id ${demo.productId}, from ${demo.source}.`;
  about.append(summary, text);

  const render = (): void => {
    const signedIn = deps.signedIn();
    const running = demo.running();
    block.dataset.demo = running ? "running" : "stopped";
    start.hidden = running;
    start.disabled = !signedIn;
    stop.hidden = !running;
    restart.hidden = !running;
    status.textContent = running
      ? `Running as ${demo.productId}. Its rooms are in Chat.`
      : signedIn
        ? "Stopped."
        : "Sign in with a wallet to start it.";
  };

  block.append(label, actions, status, about);
  deps.section.append(block);
  render();
  return { render };
}
