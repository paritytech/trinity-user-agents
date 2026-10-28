/**
 * The tabs under Chat's title: Chats, the conversations, and Inbox, the
 * requests products send. One element, put at the top of whichever of the
 * two screens shows, so both read the same selection and the same count.
 */

export type ChatTab = "chats" | "inbox";

const TABS: readonly { tab: ChatTab; label: string; controls: string }[] = [
  { tab: "chats", label: "Chats", controls: "chat-screen" },
  { tab: "inbox", label: "Inbox", controls: "notifications-screen" },
];

export class ChatTabs {
  readonly root = document.createElement("div");
  private readonly buttons = new Map<ChatTab, HTMLButtonElement>();
  private readonly badge = document.createElement("span");

  constructor(onSelect: (tab: ChatTab) => void) {
    this.root.id = "chat-tabs";
    this.root.className = "chat-tabs";
    this.root.setAttribute("role", "tablist");
    this.root.setAttribute("aria-label", "Chat");
    this.badge.className = "chat-tab-badge";
    this.badge.dataset.badge = "";
    this.badge.hidden = true;
    for (const { tab, label, controls } of TABS) {
      const button = document.createElement("button");
      button.type = "button";
      button.setAttribute("role", "tab");
      button.setAttribute("aria-controls", controls);
      button.dataset.chatTab = tab;
      button.dataset.focus = `chat-tab-${tab}`;
      button.append(label);
      if (tab === "inbox") button.append(this.badge);
      button.addEventListener("click", () => onSelect(tab));
      this.buttons.set(tab, button);
      this.root.append(button);
    }
    this.root.addEventListener("keydown", (event) => {
      const order = [...this.buttons.keys()];
      const current = order.findIndex(
        (tab) => this.buttons.get(tab) === document.activeElement,
      );
      if (current === -1) return;
      const next =
        event.key === "ArrowRight"
          ? order[(current + 1) % order.length]
          : event.key === "ArrowLeft"
            ? order[(current + order.length - 1) % order.length]
            : event.key === "Home"
              ? order[0]
              : event.key === "End"
                ? order[order.length - 1]
                : null;
      if (next === null) return;
      event.preventDefault();
      onSelect(next);
      this.focus(next);
    });
  }

  render(selected: ChatTab, unseen: number): void {
    for (const [tab, button] of this.buttons) {
      button.setAttribute("aria-selected", String(tab === selected));
      button.tabIndex = tab === selected ? 0 : -1;
    }
    this.badge.hidden = unseen === 0;
    this.badge.textContent = String(unseen);
    this.buttons
      .get("inbox")!
      .setAttribute(
        "aria-label",
        unseen === 0 ? "Inbox" : `Inbox, ${unseen} new`,
      );
  }

  focus(tab: ChatTab): void {
    this.buttons.get(tab)?.focus();
  }
}
