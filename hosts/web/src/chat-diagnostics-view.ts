import {
  describeEvent,
  type ChatDiagnostics,
  type ChatEvent,
} from "./chat/diagnostics.js";

/**
 * The inspector's Chat view: the chat engine's events, newest last. It is a
 * separate list from the TrUAPI traces, because none of these events is a
 * TrUAPI frame. It redraws only while shown.
 */

const LABELS: Record<ChatEvent["type"], string> = {
  lifecycle: "engine",
  subscription: "subscribe",
  "set-aside": "set aside",
  request: "request",
  lookup: "lookup",
  session: "session",
  submit: "submit",
  "action-failed": "failed",
};

/** Events that report a failure or a refusal. */
function isProblem(event: ChatEvent): boolean {
  switch (event.type) {
    case "lifecycle":
      return event.state === "failed";
    case "set-aside":
    case "action-failed":
      return true;
    case "request":
      return event.problem !== undefined;
    case "lookup":
      return event.outcome !== "found";
    case "submit":
      return event.result !== "stored";
    default:
      return false;
  }
}

const time = (at: number): string =>
  new Date(at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });

function row(event: ChatEvent): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "chat-diag-row";
  item.dataset.type = event.type;
  if (isProblem(event)) item.dataset.problem = "true";
  const when = document.createElement("time");
  when.dateTime = new Date(event.at).toISOString();
  when.textContent = time(event.at);
  const label = document.createElement("span");
  label.className = "chat-diag-type";
  label.textContent = LABELS[event.type];
  const text = document.createElement("span");
  text.className = "chat-diag-text";
  text.textContent = describeEvent(event);
  item.append(when, label, text);
  return item;
}

export interface ChatDiagnosticsView {
  show(visible: boolean): void;
}

export function mountChatDiagnostics(
  root: HTMLElement,
  diagnostics: ChatDiagnostics,
): ChatDiagnosticsView {
  const bar = document.createElement("div");
  bar.className = "chat-diag-bar";
  const count = document.createElement("span");
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "chat-diag-clear";
  clear.textContent = "Clear";
  clear.addEventListener("click", () => diagnostics.clear());
  bar.append(count, clear);
  const list = document.createElement("ol");
  list.className = "chat-diag-list";
  root.append(bar, list);

  let visible = false;
  const draw = () => {
    if (!visible) return;
    const atEnd = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
    const events = diagnostics.list();
    count.textContent =
      events.length === 0
        ? "No chat events yet. Metadata only: no keys, message text or tokens."
        : `${events.length} event${events.length === 1 ? "" : "s"} · metadata only, this session`;
    list.replaceChildren(...events.map(row));
    if (atEnd) list.scrollTop = list.scrollHeight;
  };
  diagnostics.onChange(draw);
  return {
    show(next) {
      visible = next;
      root.hidden = !next;
      draw();
    },
  };
}
