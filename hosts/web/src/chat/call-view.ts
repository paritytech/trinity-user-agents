import type { CallSupport, CallView } from "./provider.js";

/**
 * The call a chat provider holds, over whatever screen is in front: the
 * person, the call's state, both videos, and the buttons that state allows.
 * It is the host's own UI. It opens nothing itself: Accept and the call
 * buttons go to the provider, which opens the camera and microphone.
 */

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(
  className: string,
  label: string,
  action: string,
  onClick: () => void,
): HTMLButtonElement {
  const node = el("button", className, label);
  node.type = "button";
  node.dataset.action = action;
  node.addEventListener("click", onClick);
  return node;
}

const STATE_LINE: Record<CallView["state"], string> = {
  calling: "Calling…",
  ringing: "Ringing…",
  incoming: "Incoming call",
  connecting: "Connecting…",
  connected: "Connected",
  ended: "Call ended",
};

/** How long an ended call stays on screen. */
const ENDED_MS = 2_000;

export class CallOverlay {
  readonly root = el("div", "chat-call");
  private readonly remote = el("video", "chat-call-remote");
  private readonly local = el("video", "chat-call-local");
  private readonly head = el("div", "chat-call-head");
  private readonly actions = el("div", "chat-call-actions");
  private readonly sources = new Map<CallSupport, () => void>();
  private dismissTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    this.root.hidden = true;
    this.root.setAttribute("role", "dialog");
    this.root.setAttribute("aria-label", "Call");
    for (const video of [this.remote, this.local]) {
      video.autoplay = true;
      video.playsInline = true;
    }
    this.local.muted = true;
    const stage = el("div", "chat-call-stage");
    stage.append(this.remote, this.local);
    // The videos stay in place: moving a playing element pauses it.
    this.root.append(this.head, stage, this.actions);
  }

  /** Follow the calls of these providers, and only these. */
  follow(sources: readonly CallSupport[]): void {
    for (const [source, stop] of this.sources)
      if (!sources.includes(source)) {
        stop();
        this.sources.delete(source);
      }
    for (const source of sources)
      if (!this.sources.has(source))
        this.sources.set(
          source,
          source.subscribe(() => this.draw()),
        );
    this.draw();
  }

  private current(): { source: CallSupport; call: CallView } | null {
    for (const source of this.sources.keys()) {
      const call = source.current();
      if (call !== null) return { source, call };
    }
    return null;
  }

  draw(): void {
    const held = this.current();
    if (held === null) {
      this.root.hidden = true;
      this.head.replaceChildren();
      this.actions.replaceChildren();
      this.remote.srcObject = this.local.srcObject = null;
      return;
    }
    const { source, call } = held;
    this.root.hidden = false;
    this.root.dataset.state = call.state;
    this.root.dataset.video = String(call.video);
    if (this.remote.srcObject !== call.remote)
      this.remote.srcObject = call.remote;
    if (this.local.srcObject !== call.local) this.local.srcObject = call.local;
    this.remote.hidden = !call.video || !call.remoteCamera;
    this.local.hidden = !call.video || call.local === null || !call.cameraOn;

    const head = this.head;
    head.replaceChildren(
      el("p", "chat-call-title", call.title),
      el(
        "p",
        "chat-call-state",
        `${call.video ? "Video" : "Audio"} · ${STATE_LINE[call.state]}`,
      ),
    );
    if (call.problem !== null)
      head.append(el("p", "chat-call-problem", call.problem));
    if (call.relay && call.state !== "ended" && call.state !== "connected")
      head.append(el("p", "chat-call-relay", call.relay));

    const actions = this.actions;
    actions.replaceChildren();
    if (call.state === "incoming") {
      actions.append(
        button("chat-pill", "Decline", "decline-call", () => source.hangUp()),
        button("chat-pill chat-pill-primary", "Accept", "accept-call", () => {
          void source.accept();
        }),
      );
    } else if (call.state !== "ended") {
      actions.append(
        button("chat-pill", call.muted ? "Unmute" : "Mute", "mute", () =>
          source.setMuted(!call.muted),
        ),
      );
      if (call.video)
        actions.append(
          button(
            "chat-pill",
            call.cameraOn ? "Camera off" : "Camera on",
            "camera",
            () => source.setCamera(!call.cameraOn),
          ),
        );
      actions.append(
        button("chat-pill chat-call-end", "Hang up", "hang-up", () =>
          source.hangUp(),
        ),
      );
    }

    if (call.state === "ended" && this.dismissTimer === null)
      this.dismissTimer = setTimeout(() => {
        this.dismissTimer = null;
        source.dismiss();
      }, ENDED_MS);
  }

  dispose(): void {
    for (const stop of this.sources.values()) stop();
    this.sources.clear();
    if (this.dismissTimer !== null) clearTimeout(this.dismissTimer);
    this.root.remove();
  }
}
