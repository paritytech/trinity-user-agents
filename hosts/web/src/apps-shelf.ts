/**
 * The dock's Apps entry and its shelf, as the native TabBarShelfView draws
 * it: the bar's glass grows upward, with a row of host icons on the right
 * (the Inbox, the core's state, Settings) and the products open in this tab
 * under it as a grid of round icons. The one in front is marked. A long
 * press on a product, or its close button, closes it. The shelf draws what
 * it is given and owns no state but whether it is open.
 */

/** A product open in this tab. */
export interface ShelfApp {
  /** What the host calls it when the shelf hands it back. */
  key: string;
  name: string;
  productId: string;
  /** In front, rather than running behind a host screen. */
  front: boolean;
}

export interface ShelfView {
  apps: readonly ShelfApp[];
  /** Null when this host offers no Inbox. */
  inbox: { unseen: number; unseenFor(productId: string): number } | null;
  core: {
    network: string;
    state: "starting" | "ready" | "failed";
    /** The full status line, as the menu shows it. */
    detail: string;
  };
}

export interface ShelfActions {
  inbox(): void;
  settings(): void;
  open(key: string): void;
  close(key: string): void;
}

const CORE_LABEL: Record<ShelfView["core"]["state"], string> = {
  starting: "Starting…",
  ready: "Core ready",
  failed: "Core failed",
};

const INBOX_ICON =
  '<svg viewBox="0 0 28 28" width="28" height="28" aria-hidden="true"><path class="line" stroke-linejoin="round" d="M4 15.5l3-9.2A2.5 2.5 0 0 1 9.4 4.6h9.2a2.5 2.5 0 0 1 2.4 1.7l3 9.2V21a2.5 2.5 0 0 1-2.5 2.5h-15A2.5 2.5 0 0 1 4 21z"/><path class="fill" d="M3.9 14.4h6.8l1.3 2.5h4l1.3-2.5h6.8V21a2.5 2.5 0 0 1-2.5 2.5h-15A2.5 2.5 0 0 1 3.9 21z"/></svg>';
const STATUS_ICON =
  '<svg viewBox="0 0 28 28" width="28" height="28" aria-hidden="true"><circle cx="14" cy="14" r="11" class="ring"/><circle cx="14" cy="14" r="5" class="core"/></svg>';
const SETTINGS_ICON =
  '<svg viewBox="0 0 28 28" width="28" height="28" aria-hidden="true"><g transform="translate(2.625,2.625)"><path fill="currentColor" fill-rule="evenodd" d="M11.1744 0C10.105 0 9.19242 0.77309 9.01662 1.82788L8.90982 2.46869C8.8552 2.79644 8.61825 3.06142 8.30707 3.17794C8.11779 3.24881 7.93162 3.32605 7.74882 3.4094C7.44635 3.54732 7.09125 3.52756 6.82075 3.33435L6.29182 2.95654C5.42167 2.335 4.2297 2.43365 3.47357 3.18979L3.18981 3.47354C2.43368 4.22968 2.33503 5.42164 2.95657 6.2918L3.33436 6.82071C3.52758 7.09122 3.54733 7.44632 3.40942 7.74879C3.32606 7.93159 3.24882 8.11777 3.17794 8.30706C3.06143 8.61823 2.79645 8.85518 2.46869 8.90981L1.82788 9.01661C0.773091 9.19241 0 10.105 0 11.1743V11.5756C0 12.645 0.77309 13.5576 1.82788 13.7334L2.46869 13.8402C2.79644 13.8948 3.06142 14.1318 3.17794 14.4429C3.24881 14.6322 3.32605 14.8184 3.4094 15.0012C3.54731 15.3036 3.52756 15.6587 3.33434 15.9292L2.95652 16.4582C2.33498 17.3284 2.43363 18.5203 3.18976 19.2765L3.47352 19.5602C4.22965 20.3163 5.42162 20.415 6.29177 19.7935L6.82072 19.4156C7.09123 19.2224 7.44633 19.2027 7.74879 19.3406C7.9316 19.4239 8.11777 19.5012 8.30706 19.5721C8.61823 19.6886 8.85518 19.9536 8.90981 20.2813L9.01661 20.9221C9.19241 21.9769 10.105 22.75 11.1743 22.75H11.5756C12.645 22.75 13.5576 21.9769 13.7334 20.9221L13.8402 20.2813C13.8948 19.9536 14.1318 19.6886 14.4429 19.5721C14.6322 19.5012 14.8184 19.4239 15.0012 19.3406C15.3036 19.2027 15.6587 19.2224 15.9292 19.4157L16.4582 19.7935C17.3283 20.415 18.5203 20.3163 19.2764 19.5602L19.5602 19.2765C20.3163 18.5203 20.415 17.3284 19.7934 16.4582L19.4156 15.9293C19.2224 15.6588 19.2027 15.3037 19.3406 15.0012C19.4239 14.8184 19.5012 14.6322 19.5721 14.4429C19.6886 14.1318 19.9536 13.8948 20.2813 13.8402L20.9221 13.7334C21.9769 13.5576 22.75 12.645 22.75 11.5757V11.1744C22.75 10.105 21.9769 9.19242 20.9221 9.01662L20.2813 8.90982C19.9536 8.8552 19.6886 8.61825 19.5721 8.30707C19.5012 8.11779 19.4239 7.93163 19.3406 7.74883C19.2027 7.44636 19.2224 7.09126 19.4157 6.82076L19.7935 6.2918C20.415 5.42164 20.3164 4.22968 19.5602 3.47354L19.2765 3.18979C18.5203 2.43365 17.3284 2.335 16.4582 2.95654L15.9293 3.33436C15.6588 3.52758 15.3037 3.54733 15.0012 3.40942C14.8184 3.32606 14.6322 3.24882 14.4429 3.17794C14.1318 3.06143 13.8948 2.79645 13.8402 2.46869L13.7334 1.82788C13.5576 0.773091 12.645 0 11.5757 0H11.1744ZM11.375 15.75C13.7912 15.75 15.75 13.7912 15.75 11.375C15.75 8.95875 13.7912 7 11.375 7C8.95875 7 7 8.95875 7 11.375C7 13.7912 8.95875 15.75 11.375 15.75Z"/></g></svg>';
/** How long a press on a product takes to close it. */
const LONG_PRESS_MS = 550;

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

function countBadge(count: number, label: string): HTMLElement {
  const badge = el("span", "shelf-badge", String(count));
  badge.dataset.badge = "";
  badge.hidden = count === 0;
  badge.setAttribute("aria-label", label);
  return badge;
}

export class AppsShelf {
  private view: ShelfView | null = null;

  constructor(
    private readonly toggle: HTMLButtonElement,
    private readonly root: HTMLElement,
    private readonly actions: ShelfActions,
  ) {
    toggle.setAttribute("aria-controls", root.id);
    toggle.setAttribute("aria-expanded", "false");
    toggle.addEventListener("click", () =>
      this.isOpen() ? this.close(true) : this.open(),
    );
    for (const node of [toggle, root])
      node.addEventListener("keydown", (event) => {
        if (event.key !== "Escape" || !this.isOpen()) return;
        event.preventDefault();
        event.stopPropagation();
        this.close(true);
      });
    document.addEventListener(
      "pointerdown",
      (event) => {
        const target = event.target as Node | null;
        if (
          this.isOpen() &&
          target !== null &&
          !root.contains(target) &&
          !toggle.contains(target)
        )
          this.close(false);
      },
      true,
    );
  }

  isOpen(): boolean {
    return !this.root.hidden;
  }

  open(): void {
    this.root.hidden = false;
    document.body.dataset.shelf = "open";
    this.toggle.setAttribute("aria-expanded", "true");
    this.draw();
    this.root.querySelector<HTMLElement>("button")?.focus();
  }

  /** Close the shelf, and give focus back to Apps when it was inside. */
  close(returnFocus: boolean): void {
    if (!this.isOpen()) return;
    const inside = this.root.contains(document.activeElement);
    this.root.hidden = true;
    document.body.dataset.shelf = "closed";
    this.toggle.setAttribute("aria-expanded", "false");
    if (returnFocus || inside) this.toggle.focus();
  }

  render(view: ShelfView): void {
    this.view = view;
    const open = view.apps.length;
    const unseen = view.inbox?.unseen ?? 0;
    // The count sits inside the Apps glyph, as in the app; none while empty.
    const count = this.toggle.querySelector("[data-badge]");
    if (count !== null) {
      count.textContent = open === 0 ? "" : open > 99 ? "99+" : String(open);
      count.setAttribute("font-size", open > 9 ? "8.5" : "11");
    }
    const dot = this.toggle.querySelector<HTMLElement>("[data-inbox-dot]");
    if (dot !== null) dot.hidden = unseen === 0;
    this.toggle.setAttribute(
      "aria-label",
      `Apps, ${open} open${unseen === 0 ? "" : `. Inbox has ${unseen} new`}`,
    );
    this.toggle.title = "Apps: open products, Inbox, status and Settings";
    if (this.isOpen()) this.draw();
  }

  private draw(): void {
    const view = this.view;
    if (view === null) return;
    const focused = document.activeElement as HTMLElement | null;
    const focusKey = this.root.contains(focused)
      ? focused?.dataset.focus
      : undefined;

    const tools = el("div", "shelf-tools");
    if (view.inbox !== null) {
      const unseen = view.inbox.unseen;
      const inbox = this.button("shelf-tool shelf-inbox", "shelf-inbox", () => {
        this.close(false);
        this.actions.inbox();
      });
      inbox.innerHTML = INBOX_ICON;
      inbox.append(countBadge(unseen, `${unseen} new`));
      inbox.setAttribute(
        "aria-label",
        unseen === 0 ? "Inbox" : `Inbox, ${unseen} new`,
      );
      inbox.title = "Inbox";
      tools.append(inbox);
    }
    const status = el("span", "shelf-tool shelf-status");
    status.dataset.state = view.core.state;
    status.innerHTML = STATUS_ICON;
    const statusLine = `${view.core.network} · ${CORE_LABEL[view.core.state]}`;
    status.setAttribute("role", "img");
    status.setAttribute("aria-label", statusLine);
    status.title = `${statusLine}\n${view.core.detail}`;
    tools.append(status);
    const settings = this.button(
      "shelf-tool shelf-settings",
      "shelf-settings",
      () => {
        this.close(false);
        this.actions.settings();
      },
    );
    settings.innerHTML = SETTINGS_ICON;
    settings.setAttribute("aria-label", "Settings");
    settings.title = "Settings";
    tools.append(settings);

    const apps: HTMLElement =
      view.apps.length === 0
        ? el("p", "shelf-empty", "No open apps")
        : el("ul", "shelf-apps");
    if (view.apps.length > 0) apps.setAttribute("aria-label", "Open apps");
    for (const app of view.apps) apps.append(this.app(app, view));
    this.root.replaceChildren(tools, apps);

    if (focusKey !== undefined)
      (
        this.root.querySelector<HTMLElement>(`[data-focus="${focusKey}"]`) ??
        this.root.querySelector<HTMLElement>("button")
      )?.focus();
  }

  private app(app: ShelfApp, view: ShelfView): HTMLElement {
    const item = el("li", "shelf-app");
    item.dataset.app = app.key;
    item.dataset.productId = app.productId;
    item.dataset.front = String(app.front);
    const unseen = view.inbox?.unseenFor(app.productId) ?? 0;
    let pressed: ReturnType<typeof setTimeout> | null = null;
    let longPressed = false;
    const open = this.button("shelf-app-open", "shelf-open", () => {
      if (longPressed) return;
      if (app.front) this.close(true);
      else {
        this.close(false);
        this.actions.open(app.key);
      }
    });
    open.dataset.focus = `open:${app.key}`;
    const icon = el(
      "span",
      "shelf-app-icon",
      app.name
        .replace(/^[a-z]+:\/\//i, "")
        .charAt(0)
        .toUpperCase(),
    );
    icon.setAttribute("aria-hidden", "true");
    icon.append(countBadge(unseen, `${unseen} new in the Inbox`));
    open.append(icon, el("span", "shelf-app-name", app.name));
    open.setAttribute(
      "aria-label",
      `${app.name}, ${app.front ? "in front" : "running behind"}${unseen === 0 ? "" : `, ${unseen} new in the Inbox`}`,
    );
    open.title = `${app.productId}. Long press to close.`;
    const cancel = () => {
      if (pressed !== null) clearTimeout(pressed);
      pressed = null;
    };
    open.addEventListener("pointerdown", () => {
      longPressed = false;
      cancel();
      pressed = setTimeout(() => {
        longPressed = true;
        this.actions.close(app.key);
      }, LONG_PRESS_MS);
    });
    for (const type of ["pointerup", "pointerleave", "pointercancel"])
      open.addEventListener(type, cancel);
    open.addEventListener("contextmenu", (event) => event.preventDefault());
    const close = this.button("shelf-app-close", "shelf-close", () =>
      this.actions.close(app.key),
    );
    close.dataset.focus = `close:${app.key}`;
    close.textContent = "×";
    close.setAttribute("aria-label", `Close ${app.name}`);
    close.title = `Close ${app.name}`;
    item.append(open, close);
    return item;
  }

  private button(
    className: string,
    action: string,
    onClick: () => void,
  ): HTMLButtonElement {
    const node = el("button", className);
    node.type = "button";
    node.dataset.action = action;
    node.dataset.focus ??= action;
    node.addEventListener("click", onClick);
    return node;
  }
}
