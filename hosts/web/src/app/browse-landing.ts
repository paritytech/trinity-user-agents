import { avatar } from "../chat/conversation-view.js";
import { ago } from "../notifications/screen.js";

/** A product the landing lists under Recent. */
export interface LandingRecent {
  /** The address as the search field shows it. */
  label: string;
  /** When it was last opened, in milliseconds since the epoch. */
  at: number;
  open(): void;
}

export interface BrowseLandingDeps {
  root: HTMLElement;
  /** The Browse app's address as shown, or null when this network publishes none. */
  catalog(): string | null;
  openCatalog(): void;
  /** The recent products, newest first. */
  recents(): LandingRecent[];
}

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

/** A list row that opens something: an initial avatar, a title and a line under it. */
function row(
  title: string,
  line: string,
  action: string,
  onClick: () => void,
): HTMLLIElement {
  const item = el("li", "");
  const node = el("button", "chat-row chat-row-button");
  node.type = "button";
  node.dataset.action = action;
  node.addEventListener("click", onClick);
  const text = el("span", "chat-row-text");
  text.append(
    el("span", "chat-row-title", title),
    el("span", "chat-row-preview", line),
  );
  node.append(avatar({ icon: null, seed: title, title }), text);
  item.append(node);
  return item;
}

/**
 * What Browse shows with no product in front: the Browse app, which lists
 * the apps on the network, and the recent products. Nothing opens until the
 * user chooses; the Docklet's search opens anything else.
 */
export class BrowseLanding {
  constructor(private readonly deps: BrowseLandingDeps) {}

  show(visible: boolean): void {
    this.deps.root.hidden = !visible;
    if (visible) this.render();
  }

  render(): void {
    const { root } = this.deps;
    if (root.hidden) return;
    const bar = el("header", "chat-bar");
    bar.append(el("h2", "chat-title", "Browse"));
    const body = el("div", "chat-body");

    const catalog = this.deps.catalog();
    if (catalog !== null) {
      const list = el("ul", "chat-list");
      list.append(
        row("Browse apps", catalog, "open-catalog", () =>
          this.deps.openCatalog(),
        ),
      );
      body.append(list);
    }

    const recents = this.deps.recents();
    if (recents.length > 0) {
      const section = el("section", "chat-section browse-recents");
      section.append(el("h3", "chat-section-title", "Recent"));
      const list = el("ul", "chat-list");
      for (const recent of recents) {
        const item = row(
          recent.label,
          ago(recent.at, Date.now()),
          "open-recent",
          () => recent.open(),
        );
        item.firstElementChild!.setAttribute("data-address", recent.label);
        list.append(item);
      }
      section.append(list);
      body.append(section);
    }
    if (catalog === null && recents.length === 0) {
      const empty = el("div", "chat-empty");
      empty.append(
        el("p", "chat-empty-title", "No recent apps"),
        el("p", "chat-empty-body", "Open one with the search button."),
      );
      body.append(empty);
    }
    root.replaceChildren(bar, body);
  }
}
