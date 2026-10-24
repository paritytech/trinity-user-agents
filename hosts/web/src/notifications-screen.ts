import { avatar } from "./conversation-view.js";
import {
  statusOf,
  type HostNotification,
  type NotificationKind,
  type NotificationStatus,
  type NotificationStore,
} from "./notifications.js";

/**
 * PROTOTYPE. The Notifications screen: requests that wait for the user's
 * answer, then the ones already answered or expired. A request opens in a
 * detail the host draws, with who sent it, through which product, what for,
 * and Accept and Decline. Opening a request marks it seen; only an answer or
 * its expiry ends it.
 *
 * Only the local demo puts requests here, so the screen says so on every
 * view, and its answers go nowhere but this list.
 */

export interface NotificationsDemoControl {
  /** Put the demo's requests in this wallet's list, in place of earlier ones. */
  seed(): void;
  /** Take the demo's requests out of this wallet's list. */
  clear(): void;
}

const KIND_LABEL: Record<NotificationKind, string> = {
  invitation: "Chat invitation",
  sharing: "Profile sharing request",
};

const KIND_LINE: Record<NotificationKind, string> = {
  invitation: "invites you to chat",
  sharing: "asks to see your profile",
};

const STATUS_LABEL: Record<NotificationStatus, string> = {
  pending: "Waiting for you",
  accepted: "Accepted",
  declined: "Declined",
  expired: "Expired",
};

/** How often relative times are drawn again while the screen shows. */
const REFRESH_MS = 30_000;
/** Longest delay `setTimeout` takes. */
const MAX_DELAY_MS = 2 ** 31 - 1;

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

/** `at` relative to `now`, as a list shows it. */
export function ago(at: number, now: number): string {
  const minutes = Math.round((now - at) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h`;
  return `${Math.round(hours / 24)} d`;
}

/** How far `at` is from `now`, in words, for an expiry. */
export function until(at: number, now: number): string {
  const minutes = Math.ceil((at - now) / 60_000);
  if (minutes < 60) return `in ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `in ${hours} h`;
  const days = Math.round(hours / 24);
  return days === 1 ? "in 1 day" : `in ${days} days`;
}

const when = (at: number): string =>
  new Date(at).toLocaleString([], {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });

export class NotificationsScreen {
  private view: { kind: "list" } | { kind: "detail"; id: string } = {
    kind: "list",
  };
  private problem: string | null = null;
  private visible = false;
  private expiryTimer: ReturnType<typeof setTimeout> | null = null;
  private refreshTimer: ReturnType<typeof setInterval> | null = null;
  /** The row to give focus to when the list is drawn again. */
  private returnTo: string | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly store: NotificationStore,
    private readonly onBadge: (unseen: number) => void,
    private readonly demo?: NotificationsDemoControl,
  ) {
    store.onChange(() => {
      if (!store.scoped()) this.view = { kind: "list" };
      this.render();
    });
    this.render();
  }

  show(visible: boolean): void {
    this.visible = visible;
    this.root.hidden = !visible;
    if (this.refreshTimer !== null) clearInterval(this.refreshTimer);
    this.refreshTimer = visible
      ? setInterval(() => this.render(), REFRESH_MS)
      : null;
    this.render();
  }

  private render(): void {
    this.onBadge(this.store.unseen());
    this.scheduleExpiry();
    if (!this.visible) return;
    const item =
      this.view.kind === "detail" ? this.store.get(this.view.id) : undefined;
    if (this.view.kind === "detail" && item === undefined)
      this.view = { kind: "list" };
    this.root.dataset.view = this.view.kind;
    this.root.replaceChildren(
      ...(item === undefined ? this.drawList() : this.drawDetail(item)),
    );
    if (item === undefined && this.returnTo !== null) {
      this.root
        .querySelector<HTMLElement>(
          `[data-notification="${CSS.escape(this.returnTo)}"]`,
        )
        ?.focus();
      this.returnTo = null;
    }
  }

  /** Draw again when the next pending request expires, so it leaves pending on time. */
  private scheduleExpiry(): void {
    if (this.expiryTimer !== null) clearTimeout(this.expiryTimer);
    this.expiryTimer = null;
    const next = this.store.nextExpiry();
    if (next === null) return;
    const delay = Math.min(
      Math.max(0, next - this.store.now()) + 50,
      MAX_DELAY_MS,
    );
    this.expiryTimer = setTimeout(() => this.render(), delay);
  }

  /** What the demo data is, and, on the list, the demo's own controls. */
  private demoNote(controls: boolean): HTMLElement {
    const note = el("section", "notification-demo");
    note.append(
      el(
        "p",
        "notification-demo-text",
        "Demo data, made up by this host. Accepting or declining changes only this list: no request is sent, nothing is shared, and no permission or contact is created.",
      ),
    );
    if (controls && this.demo !== undefined && this.store.scoped()) {
      const demo = this.demo;
      const actions = el("div", "notification-demo-actions");
      const empty = this.store.list().length === 0;
      actions.append(
        button(
          empty ? "chat-pill chat-pill-primary" : "chat-pill",
          empty ? "Add demo notifications" : "Reset demo",
          "notifications-demo-seed",
          () => demo.seed(),
        ),
      );
      if (!empty)
        actions.append(
          button("chat-pill", "Clear", "notifications-demo-clear", () =>
            demo.clear(),
          ),
        );
      note.append(actions);
    }
    return note;
  }

  private drawList(): HTMLElement[] {
    const bar = el("header", "chat-bar");
    bar.append(el("h2", "chat-title", "Notifications"));
    const body = el("div", "chat-body");
    if (!this.store.scoped()) {
      body.append(
        this.empty(
          "Sign in to see notifications",
          "Requests sent to this wallet show here.",
        ),
      );
      return [bar, body];
    }
    body.append(this.demoNote(true));
    const now = this.store.now();
    const items = this.store.list();
    const waiting = items.filter((item) => statusOf(item, now) === "pending");
    const earlier = items.filter((item) => statusOf(item, now) !== "pending");
    if (items.length === 0)
      body.append(
        this.empty(
          "No notifications",
          "Chat invitations and profile sharing requests show here.",
        ),
      );
    for (const [title, group, name] of [
      ["Requests", waiting, "pending"],
      ["Earlier", earlier, "earlier"],
    ] as const) {
      if (group.length === 0) continue;
      const section = el("section", "chat-section");
      section.dataset.group = name;
      const list = el("ul", "chat-list notification-list");
      for (const item of group) list.append(this.row(item, now));
      section.append(el("h3", "chat-section-title", title), list);
      body.append(section);
    }
    return [bar, body];
  }

  private empty(title: string, text: string): HTMLElement {
    const node = el("div", "chat-empty");
    node.append(
      el("p", "chat-empty-title", title),
      el("p", "chat-empty-body", text),
    );
    return node;
  }

  private row(item: HostNotification, now: number): HTMLElement {
    const status = statusOf(item, now);
    const entry = el("li", "");
    const row = el("button", "chat-row chat-row-button notification-row");
    row.type = "button";
    row.dataset.notification = item.id;
    row.dataset.status = status;
    row.dataset.seen = String(item.seenAt !== null);
    const text = el("span", "chat-row-text");
    const title = el("span", "chat-row-title notification-title");
    title.append(
      el("strong", "", item.sender.name),
      ` ${KIND_LINE[item.kind]}`,
    );
    text.append(
      title,
      el("span", "chat-row-preview", item.purpose),
      el(
        "span",
        "chat-row-preview notification-via",
        `via ${item.product.name}${status === "pending" ? ` · expires ${until(item.expiresAt, now)}` : ""}`,
      ),
    );
    const side = el("span", "chat-row-side");
    side.append(el("span", "chat-row-time", ago(item.receivedAt, now)));
    if (status !== "pending")
      side.append(el("span", `notification-status`, STATUS_LABEL[status]));
    if (item.seenAt === null) {
      const dot = el("span", "notification-unseen");
      dot.setAttribute("aria-hidden", "true");
      side.append(dot);
    }
    row.setAttribute(
      "aria-label",
      `${item.seenAt === null ? "New. " : ""}${item.sender.name} ${KIND_LINE[item.kind]}, via ${item.product.name}. ${STATUS_LABEL[status]}.`,
    );
    row.append(
      avatar({ icon: null, seed: item.sender.handle, title: item.sender.name }),
      text,
      side,
    );
    row.addEventListener("click", () => this.open(item.id));
    entry.append(row);
    return entry;
  }

  private open(id: string): void {
    this.problem = null;
    this.view = { kind: "detail", id };
    // Marking it seen draws the screen again, showing the detail.
    this.store.markSeen(id);
    this.render();
    this.root.querySelector<HTMLElement>(".notification-heading")?.focus();
  }

  private back(): void {
    if (this.view.kind === "detail") this.returnTo = this.view.id;
    this.view = { kind: "list" };
    this.problem = null;
    this.render();
  }

  private answer(id: string, accept: boolean): void {
    const result = accept ? this.store.accept(id) : this.store.decline(id);
    this.problem = result.ok ? null : result.reason;
    this.render();
  }

  private drawDetail(item: HostNotification): HTMLElement[] {
    const now = this.store.now();
    const status = statusOf(item, now);
    const bar = el("header", "chat-bar chat-bar-small");
    const back = button("chat-icon-button", "‹", "notifications-back", () =>
      this.back(),
    );
    back.setAttribute("aria-label", "Back to notifications");
    const heading = el(
      "h2",
      "chat-title chat-title-small notification-heading",
      KIND_LABEL[item.kind],
    );
    heading.tabIndex = -1;
    bar.append(back, heading);

    const body = el("div", "chat-body notification-detail");
    body.dataset.notification = item.id;
    body.dataset.status = status;
    const who = el("section", "notification-who");
    const names = el("div", "notification-names");
    names.append(
      el("p", "notification-sender", item.sender.name),
      el("p", "notification-handle", item.sender.handle),
    );
    who.append(
      avatar(
        { icon: null, seed: item.sender.handle, title: item.sender.name },
        "large",
      ),
      names,
    );

    const facts = el("dl", "notification-facts");
    const fact = (term: string, value: string) =>
      facts.append(el("dt", "", term), el("dd", "", value));
    fact("Through", `${item.product.name} (${item.product.id})`);
    fact("Purpose", item.purpose);
    fact("Received", when(item.receivedAt));
    if (status === "pending")
      fact("Expires", `${when(item.expiresAt)}, ${until(item.expiresAt, now)}`);
    else if (status === "expired") fact("Expired", when(item.expiresAt));
    else if (item.state.tag !== "pending")
      fact(STATUS_LABEL[status], when(item.state.at));

    const outcome = el("section", "notification-outcome");
    if (status === "pending") {
      const actions = el("div", "chat-request-actions notification-actions");
      actions.append(
        button("chat-pill", "Decline", "notification-decline", () =>
          this.answer(item.id, false),
        ),
        button(
          "chat-pill chat-pill-primary",
          "Accept",
          "notification-accept",
          () => this.answer(item.id, true),
        ),
      );
      outcome.append(actions);
    } else
      outcome.append(
        el(
          "p",
          "notification-status notification-status-large",
          STATUS_LABEL[status],
        ),
      );
    if (this.problem !== null)
      outcome.append(el("p", "chat-error", this.problem));

    body.append(
      who,
      facts,
      outcome,
      el(
        "p",
        "chat-identity-line notification-caveat",
        "The sender's name comes from the request; this host has not checked it.",
      ),
      this.demoNote(false),
    );
    return [bar, body];
  }
}
