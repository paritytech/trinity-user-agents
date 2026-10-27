import { displayAddress } from "./address.js";
import type { NetworkConfig } from "./network-config.js";
import { avatarColor } from "./avatar-color.js";
import { matchRecents, type RecentEntry } from "./recents.js";
import type { PersonRow } from "./search-people.js";

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
  /**
   * People to offer beside the apps. While any show, the list has an Apps
   * group and a People group, and the Apps group also offers what is typed.
   */
  people?: {
    shown(): boolean;
    rows(): PersonRow[];
    status(): string | null;
  };
  /** What is typed, as the app it opens, or null when it opens none. */
  typedApp?(): string | null;
  /** Open what is typed as an app, as Enter does. */
  openTyped?(): void;
}

interface Option {
  key: string;
  item: HTMLLIElement;
  choose(): void;
}

function heading(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "recent-heading";
  item.setAttribute("role", "presentation");
  item.textContent = text;
  return item;
}

function row(name: string, note: string | null): HTMLLIElement {
  const item = document.createElement("li");
  item.setAttribute("role", "option");
  const label = document.createElement("span");
  label.className = "recent-name";
  label.textContent = name;
  item.append(label);
  if (note !== null) {
    const id = document.createElement("span");
    id.className = "recent-id";
    id.textContent = note;
    item.append(id);
  }
  return item;
}

/**
 * The suggestions under the address field: the products opened before and,
 * where the host can show a person, people by name or username.
 *
 * It opens when the field gets focus or a click, filters as you type, and
 * never loads or picks anything by itself: a row answers only when it is
 * chosen, and Enter with no row chosen opens what is typed as an app. The
 * field keeps focus while the list is used, so the keyboard keeps working.
 */
export function bindRecentsMenu(options: RecentsMenuOptions): {
  /** Close the list; true when it was open. */
  close(): boolean;
  /** Redraw the list after the text or the entries changed, while the field has focus. */
  refresh(): void;
} {
  const { input, list, network, entries, untouched, choose, people } = options;
  let shown: Option[] = [];
  let active = -1;

  const isOpen = (): boolean => !list.hidden;

  function setActive(next: number): void {
    active = next;
    shown.forEach((option, index) => {
      option.item.setAttribute("aria-selected", String(index === active));
      if (index === active) {
        input.setAttribute("aria-activedescendant", option.item.id);
        option.item.scrollIntoView({ block: "nearest" });
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

  function option(key: string, item: HTMLLIElement, run: () => void): Option {
    item.id = `address-option-${key.replace(/[^a-z0-9-]/gi, "-")}`;
    const chosen = { key, item, choose: run };
    item.addEventListener("click", () => pick(chosen));
    return chosen;
  }

  function pick(chosen: Option): void {
    // A person row may still be checked; it stays until it answers.
    if (!chosen.key.startsWith("person:")) close();
    chosen.choose();
  }

  function open(): void {
    const activeKey = shown[active]?.key;
    const recent = untouched()
      ? entries()
      : matchRecents(entries(), input.value);
    const withPeople = people?.shown() === true;
    const apps = recent.map((entry, index) => {
      const name = displayAddress(entry.address, network);
      return option(
        `app-${index}`,
        row(name, entry.entered ? `as ${entry.productId}` : null),
        () => choose(entry),
      );
    });
    const typed = withPeople ? (options.typedApp?.() ?? null) : null;
    if (
      typed !== null &&
      options.openTyped !== undefined &&
      !recent.some((entry) => displayAddress(entry.address, network) === typed)
    )
      apps.unshift(
        option("app-typed", row(`Open ${typed}`, null), options.openTyped),
      );
    const persons = withPeople
      ? people!.rows().map((person) => {
          const item = row(person.label, person.note);
          item.classList.add("recent-person");
          const avatar = document.createElement("span");
          avatar.className = "recent-avatar";
          avatar.dataset.initial = (person.label || "?")
            .slice(0, 1)
            .toUpperCase();
          avatar.style.setProperty("--avatar", avatarColor(person.avatarSeed));
          avatar.setAttribute("aria-hidden", "true");
          item.prepend(avatar);
          return option(person.key, item, person.choose);
        })
      : [];
    const status = withPeople ? people!.status() : null;
    shown = [...apps, ...persons];
    if (shown.length === 0 && status === null) {
      close();
      return;
    }
    const children: HTMLLIElement[] = [];
    if (withPeople) {
      if (apps.length > 0) children.push(heading("Apps"));
      children.push(...apps.map((entry) => entry.item));
      children.push(heading("People"));
      children.push(...persons.map((person) => person.item));
      if (status !== null) {
        const line = heading(status);
        line.className = "recent-status";
        children.push(line);
      }
    } else children.push(...apps.map((entry) => entry.item));
    list.replaceChildren(...children);
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
    setActive(shown.findIndex((entry) => entry.key === activeKey));
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
      pick(shown[active]);
    }
  });

  return {
    close,
    refresh() {
      if (isOpen() || document.activeElement === input) open();
    },
  };
}
