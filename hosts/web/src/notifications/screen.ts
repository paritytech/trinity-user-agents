import { avatar } from "../chat/conversation-view.js";
import {
  statusOf,
  type HostNotification,
  type NotificationKind,
  type NotificationStatus,
  type NotificationStore,
} from "./notifications.js";

/**
 * PROTOTYPE. The Inbox: requests that products send the user, those that wait
 * first, then earlier ones. It shows under Chat's Inbox tab; the Apps shelf
 * opens the same view. There is no native Inbox; the rows follow the native
 * Chat Requests list, and each names the product it comes through.
 *
 * - A row is seen once it shows on screen, in a visible page, and only then.
 * - A request that waits offers Dismiss, and Accept when the feature that
 *   owns it can accept it. Dismiss takes it off this list only; it answers
 *   nothing. Accept is the user's intent, handed to the request's owner; the
 *   row shows Accepted only once the owner accepted it, and the owner decides
 *   where, if anywhere, the user goes next.
 * - A request with no action stays seen and waiting until it expires.
 */

/** What the feature that owns some notifications does with them. */
export interface NotificationActions {
  /** Whether Accept applies to `item`. */
  canAccept(item: HostNotification): boolean;
  /** Hand the user's Accept to the owner. Resolves once it accepted; rejects, with a message for the user, when it did not. */
  accept(item: HostNotification): Promise<void>;
  /** The name of where an accepted `item` opens, or null when it opens nowhere. */
  destination(item: HostNotification): string | null;
  /** Ask the owner to show an accepted `item`. */
  open(item: HostNotification): void;
}

const KIND_LINE: Record<NotificationKind, string> = {
  invitation: "Chat invitation",
  sharing: "Profile request",
};

const STATUS_LABEL: Record<NotificationStatus, string> = {
  pending: "Waiting",
  accepted: "Accepted",
  declined: "Declined",
  expired: "Expired",
};

/** How often relative times are drawn again while the screen shows. */
const REFRESH_MS = 30_000;
/** Longest delay `setTimeout` takes. */
const MAX_DELAY_MS = 2 ** 31 - 1;
/** How much of a row has to show for it to count as seen. */
const SEEN_THRESHOLD = 0.6;

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

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class NotificationsScreen {
  private visible = false;
  private actions: NotificationActions | null = null;
  /** Requests whose Accept is under way. */
  private readonly accepting = new Set<string>();
  /** Why the last Accept of a request failed, by id. */
  private readonly problems = new Map<string, string>();
  private expiryTimer: ReturnType<typeof setTimeout> | null = null;
  private refreshTimer: ReturnType<typeof setInterval> | null = null;
  private observer: IntersectionObserver | null = null;
  /** Unseen rows that show on screen now. */
  private readonly inView = new Set<string>();

  constructor(
    private readonly root: HTMLElement,
    private readonly store: NotificationStore,
    private readonly onBadge: (unseen: number) => void,
    /** Drawn under the title, such as the tabs back to Chats. */
    private readonly header?: HTMLElement,
  ) {
    store.onChange(() => {
      if (!store.scoped()) {
        this.accepting.clear();
        this.problems.clear();
      }
      this.render();
    });
    document.addEventListener("visibilitychange", () => this.markShownSeen());
    this.render();
  }

  /** Let the feature that owns some notifications act on them, or no one. */
  setActions(actions: NotificationActions | null): void {
    this.actions = actions;
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
    this.observer?.disconnect();
    this.observer = null;
    this.inView.clear();
    if (!this.visible) return;
    const previous = this.root.querySelector<HTMLElement>(".chat-body");
    const scrollTop = previous?.scrollTop ?? 0;
    const focused = document.activeElement as HTMLElement | null;
    const headerFocus =
      focused !== null && this.header?.contains(focused) ? focused : null;
    const focusKey = this.root.contains(focused)
      ? {
          id: focused?.closest<HTMLElement>("[data-notification]")?.dataset
            .notification,
          action: focused?.dataset.action,
        }
      : null;

    const [bar, body] = this.drawList();
    this.root.replaceChildren(
      ...(this.header === undefined ? [bar, body] : [bar, this.header, body]),
    );
    body.scrollTop = scrollTop;
    headerFocus?.focus();
    if (focusKey?.id !== undefined && focusKey.action !== undefined)
      this.root
        .querySelector<HTMLElement>(
          `[data-notification="${CSS.escape(focusKey.id)}"] [data-action="${CSS.escape(focusKey.action)}"]`,
        )
        ?.focus();
    this.observe(body);
  }

  /** Watch the unseen rows, so each is marked seen once it shows. */
  private observe(body: HTMLElement): void {
    const unseen = body.querySelectorAll<HTMLElement>(
      '.notification-row[data-seen="false"]',
    );
    if (unseen.length === 0) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (observer !== this.observer) return;
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.notification!;
          if (entry.isIntersecting) this.inView.add(id);
          else this.inView.delete(id);
        }
        this.markShownSeen();
      },
      { root: body, threshold: SEEN_THRESHOLD },
    );
    this.observer = observer;
    for (const row of unseen) observer.observe(row);
  }

  /** Mark seen the unseen rows on screen, while the screen and the page show. */
  private markShownSeen(): void {
    if (
      !this.visible ||
      document.visibilityState !== "visible" ||
      this.inView.size === 0
    )
      return;
    this.store.markSeen(...this.inView);
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

  private drawList(): [HTMLElement, HTMLElement] {
    const bar = el("header", "chat-bar");
    bar.append(el("h2", "chat-title", "Inbox"));
    const body = el("div", "chat-body");
    if (!this.store.scoped()) {
      body.append(this.empty("Sign in to see your Inbox"));
      return [bar, body];
    }
    const now = this.store.now();
    const items = this.store.listed();
    const waiting = items.filter((item) => statusOf(item, now) === "pending");
    const earlier = items.filter((item) => statusOf(item, now) !== "pending");
    if (items.length === 0) body.append(this.empty("Your Inbox is empty"));
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

  private empty(title: string): HTMLElement {
    const node = el("div", "chat-empty");
    node.append(el("p", "chat-empty-title", title));
    return node;
  }

  private row(item: HostNotification, now: number): HTMLElement {
    const status = statusOf(item, now);
    const seen = item.seenAt !== null;
    const destination =
      status === "accepted" ? (this.actions?.destination(item) ?? null) : null;
    const row = el("li", "notification-row");
    row.dataset.notification = item.id;
    row.dataset.status = status;
    row.dataset.seen = String(seen);

    const top = el("span", "notification-row-top");
    top.append(
      el("span", "chat-row-title", item.sender.name),
      el("span", "notification-time", ago(item.receivedAt, now)),
    );
    const message = el("span", "notification-message");
    message.append(el("span", "notification-text", item.purpose));
    if (!seen) {
      const dot = el("span", "notification-unseen");
      dot.setAttribute("aria-hidden", "true");
      message.append(dot);
    }
    const meta = el("span", "notification-meta");
    const product = el("span", "notification-product", item.product.name);
    product.title = item.product.id;
    meta.append(`${KIND_LINE[item.kind]} · `, product);
    meta.append(
      status === "pending"
        ? ` · Expires ${until(item.expiresAt, now)}`
        : el("span", "notification-status", STATUS_LABEL[status]),
    );
    const text = el("span", "chat-row-text");
    text.append(top, message, meta);
    const content = [
      avatar({ icon: null, seed: item.sender.handle, title: item.sender.name }),
      text,
    ];
    const label = `${seen ? "" : "New. "}${KIND_LINE[item.kind]} from ${item.sender.name}, via ${item.product.name}. ${STATUS_LABEL[status]}.`;

    if (destination !== null) {
      const open = el("button", "chat-row chat-row-button notification-main");
      open.type = "button";
      open.dataset.action = "notification-open";
      open.setAttribute("aria-label", `${label} Open in ${destination}.`);
      open.append(...content, el("span", "notification-chevron", "›"));
      open.addEventListener("click", () => this.actions?.open(item));
      row.append(open);
    } else {
      const main = el("div", "chat-row notification-main");
      main.setAttribute("aria-label", label);
      main.setAttribute("role", "group");
      main.append(...content);
      row.append(main);
    }

    if (status === "pending") row.append(this.drawActions(item));
    const problem = this.problems.get(item.id);
    if (problem !== undefined)
      row.append(el("p", "chat-error notification-problem", problem));
    return row;
  }

  private drawActions(item: HostNotification): HTMLElement {
    const busy = this.accepting.has(item.id);
    const actions = el("div", "notification-actions");
    const dismiss = button("chat-pill", "Dismiss", "notification-dismiss", () =>
      this.dismiss(item),
    );
    dismiss.disabled = busy;
    actions.append(dismiss);
    if (this.actions?.canAccept(item)) {
      const accept = button(
        "chat-pill chat-pill-primary",
        busy ? "Accepting…" : "Accept",
        "notification-accept",
        () => void this.accept(item),
      );
      accept.disabled = busy;
      actions.append(accept);
    }
    return actions;
  }

  private dismiss(item: HostNotification): void {
    this.problems.delete(item.id);
    this.store.dismiss(item.id);
  }

  private async accept(item: HostNotification): Promise<void> {
    const actions = this.actions;
    if (actions === null || this.accepting.has(item.id)) return;
    this.accepting.add(item.id);
    this.problems.delete(item.id);
    this.render();
    try {
      await actions.accept(item);
    } catch (error) {
      this.problems.set(item.id, errorText(error));
    } finally {
      this.accepting.delete(item.id);
      this.render();
    }
  }
}
