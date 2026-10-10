import { displayAddress } from "./address.js";
import type { NetworkConfig } from "./network-config.js";
import { matchRecents, type RecentEntry } from "./recents.js";

export interface RecentsMenuOptions {
  input: HTMLInputElement;
  list: HTMLUListElement;
  /** The network the entries were opened on, so a name shows with its own TLD. */
  network: NetworkConfig;
  /** The entries to offer, newest first, for the wallet in use. */
  entries(): RecentEntry[];
  /** Whether the field still holds what is open, so every entry is offered, not only matches. */
  untouched(): boolean;
  /** An entry was chosen by click, touch or Enter. */
  choose(entry: RecentEntry): void;
}

/**
 * The history list under the address field.
 *
 * It opens when the field gets focus or a click, filters as you type, and
 * never loads anything by itself: an entry opens only when it is chosen. The
 * field keeps focus while the list is used, so the keyboard keeps working.
 */
export function bindRecentsMenu(options: RecentsMenuOptions): {
  /** Close the list; true when it was open. */
  close(): boolean;
  /** Redraw an open list after the text or the entries changed. */
  refresh(): void;
} {
  const { input, list, network, entries, untouched, choose } = options;
  let shown: RecentEntry[] = [];
  let active = -1;

  const isOpen = (): boolean => !list.hidden;

  function setActive(next: number): void {
    active = next;
    list.querySelectorAll("li").forEach((item, index) => {
      item.setAttribute("aria-selected", String(index === active));
      if (index === active) {
        input.setAttribute("aria-activedescendant", item.id);
        item.scrollIntoView({ block: "nearest" });
      }
    });
    if (active < 0) input.removeAttribute("aria-activedescendant");
  }

  function close(): boolean {
    const wasOpen = isOpen();
    list.hidden = true;
    list.replaceChildren();
    shown = [];
    active = -1;
    input.setAttribute("aria-expanded", "false");
    input.removeAttribute("aria-activedescendant");
    return wasOpen;
  }

  function open(): void {
    shown = untouched() ? entries() : matchRecents(entries(), input.value);
    if (shown.length === 0) {
      close();
      return;
    }
    list.replaceChildren(
      ...shown.map((entry, index) => {
        const item = document.createElement("li");
        item.id = `address-recent-${index}`;
        item.setAttribute("role", "option");
        const name = document.createElement("span");
        name.className = "recent-name";
        name.textContent = displayAddress(entry.address, network);
        item.append(name);
        if (entry.entered) {
          const id = document.createElement("span");
          id.className = "recent-id";
          id.textContent = `as ${entry.productId}`;
          item.append(id);
        }
        item.addEventListener("click", () => {
          close();
          choose(entry);
        });
        return item;
      }),
    );
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
    setActive(-1);
  }

  // Keeps focus in the field when a row is pressed, so the field does not blur
  // and close the list before the click lands.
  list.addEventListener("mousedown", (event) => event.preventDefault());
  input.addEventListener("focus", open);
  input.addEventListener("click", () => {
    if (!isOpen()) open();
  });
  input.addEventListener("input", open);
  input.addEventListener("blur", () => {
    close();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (!isOpen()) open();
      if (shown.length === 0) return;
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((active + step + shown.length) % shown.length);
    } else if (event.key === "Enter" && isOpen() && active >= 0) {
      event.preventDefault();
      const entry = shown[active];
      close();
      choose(entry);
    }
  });

  return {
    close,
    refresh() {
      if (isOpen()) open();
    },
  };
}
