import { avatarColor } from "./avatar-color.js";
import type { PromptField } from "./reviews.js";

/**
 * The host's own questions to the user: a permission, a confirmation, a
 * choice of contact. Each is drawn as the Polkadot app draws its bottom
 * sheets (`BottomSheetBaseLayout` on iOS): a rounded sheet at the bottom of
 * the app, over a scrim that covers the app only.
 *
 * The sheet is a modal `<dialog>`, so the browser keeps focus inside it,
 * makes the rest of the page inert and answers Escape. It sits in the top
 * layer, placed and scaled over the app's rectangle ({@link setPromptArea}),
 * so the inspector and the menu stay visible beside it. Tapping the scrim
 * gives the same answer as Escape.
 */

/** How a choice reads. A list puts `cancel` choices below the others. */
export type PromptTone = "primary" | "destructive" | "cancel";

/** One choice of a prompt and the answer it gives. */
export interface PromptChoice<T> {
  label: string;
  value: T;
  tone?: PromptTone;
  /** Draw the choice as a person: an initial avatar in the color this seed picks. */
  avatarSeed?: string;
}

export interface PromptRequest<T> {
  title: string;
  /** The product that asks, named once under the title. */
  from?: string;
  subtitle?: string;
  fields: PromptField[];
  choices: PromptChoice<T>[];
  /** The answer when the prompt is dismissed without choosing, as with Escape. */
  dismissed: T;
  /** `list` stacks the choices, one per row, for a choice among many. */
  layout?: "list";
  /** A field that filters a list's rows by their label as the user types. */
  search?: {
    /** The field's accessible name. */
    label: string;
    placeholder: string;
    /** What shows when no row matches. */
    empty: string;
    /** A second group, below the rows, filled from elsewhere as the user types. */
    lookup?: PromptLookup<T>;
  };
}

/**
 * A group of people found elsewhere for what the user typed, such as by
 * username, each a choice like the prompt's own rows.
 */
export interface PromptLookup<T> {
  /** The heading over the prompt's own rows, so the two groups read apart. */
  ownHeading: string;
  heading: string;
  /** What the group's rows are, under its heading. */
  note: string;
  /**
   * Start following what is typed. `render` draws each new state; `answer`
   * gives the prompt's answer, once, and closes it.
   */
  open(
    render: (view: PromptLookupView) => void,
    answer: (value: T) => void,
  ): {
    type(text: string): void;
    more(): void;
    close(): void;
  };
}

export interface PromptLookupView {
  /** Whether the group shows at all, as it does not before anything is typed. */
  visible: boolean;
  rows: { label: string; avatarSeed: string; choose(): void }[];
  /** Whether the rows take a tap now, as they do not while one is being checked. */
  enabled: boolean;
  /** A line under the rows, such as "Searching…" or why it failed. */
  status: string | null;
  /** Whether a next page can be asked for. */
  more: boolean;
}

let queue: Promise<unknown> = Promise.resolve();
let area: HTMLElement | null = null;
let describeRequester: (productId: string) => string | null = () => null;

/** Draw prompts over `element`, the app's rectangle, instead of the whole page. */
export function setPromptArea(element: HTMLElement | null): void {
  area = element;
}

/**
 * Say, under "Requested by <product id>", what the host knows of that
 * product beyond its id, or null for nothing. Read each time a prompt shows.
 */
export function setRequesterDescriber(
  describe: (productId: string) => string | null,
): void {
  describeRequester = describe;
}

/**
 * Ask the user and resolve with the choice.
 *
 * The core can ask several things at once. Prompts are shown one at a time in
 * the order they were asked, so no answer is given to a question the user did
 * not see.
 */
export function ask<T>(request: PromptRequest<T>): Promise<T> {
  const answer = queue.then(() => show(request));
  queue = answer.catch(() => {});
  return answer;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== "") node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Keep `dialog` over the app's rectangle, at the app's own scale, while it is open. */
function followArea(dialog: HTMLDialogElement): () => void {
  const target = area;
  if (target === null || !target.isConnected) return () => {};
  const place = () => {
    const rect = target.getBoundingClientRect();
    const width = target.offsetWidth;
    const height = target.offsetHeight;
    if (width === 0 || height === 0) {
      dialog.removeAttribute("data-area");
      return;
    }
    dialog.dataset.area = "app";
    dialog.style.setProperty("--prompt-left", `${rect.left}px`);
    dialog.style.setProperty("--prompt-top", `${rect.top}px`);
    dialog.style.setProperty("--prompt-width", `${width}px`);
    dialog.style.setProperty("--prompt-height", `${height}px`);
    dialog.style.setProperty("--prompt-scale", String(rect.width / width));
  };
  place();
  const resized = new ResizeObserver(place);
  resized.observe(target);
  resized.observe(document.body);
  const restyled = new MutationObserver(place);
  restyled.observe(target, { attributes: true, attributeFilter: ["style"] });
  window.addEventListener("resize", place);
  return () => {
    resized.disconnect();
    restyled.disconnect();
    window.removeEventListener("resize", place);
  };
}

function choiceButton<T>(
  choice: PromptChoice<T>,
  className: string,
  onChoose: (value: T) => void,
): HTMLButtonElement {
  const button = el("button", className);
  button.type = "button";
  if (choice.tone !== undefined) button.dataset.tone = choice.tone;
  if (choice.avatarSeed !== undefined)
    button.append(avatar(choice.label, choice.avatarSeed));
  button.append(el("span", "prompt-label", choice.label));
  button.addEventListener("click", () => onChoose(choice.value));
  return button;
}

function avatar(label: string, seed: string): HTMLElement {
  const initial = el("span", "prompt-avatar");
  initial.dataset.initial = (label || "?").slice(0, 1).toUpperCase();
  initial.style.setProperty("--avatar", avatarColor(seed));
  initial.setAttribute("aria-hidden", "true");
  return initial;
}

/** The lookup group's elements, redrawn on each view. Returns the redraw. */
function lookupGroup(
  lookup: PromptLookup<unknown>,
  group: HTMLElement,
  onMore: () => void,
): (view: PromptLookupView) => void {
  const list = el("div", "prompt-list");
  list.setAttribute("aria-label", lookup.heading);
  const status = el("p", "prompt-empty");
  status.setAttribute("role", "status");
  const more = el("button", "prompt-more", "Show more");
  more.type = "button";
  more.addEventListener("click", onMore);
  group.append(
    el("h3", "prompt-group", lookup.heading),
    el("p", "prompt-group-note", lookup.note),
    list,
    status,
    more,
  );
  return (view) => {
    group.hidden = !view.visible;
    list.replaceChildren(
      ...view.rows.map((row) => {
        const item = el("button", "prompt-row");
        item.type = "button";
        item.disabled = !view.enabled;
        item.append(
          avatar(row.label, row.avatarSeed),
          el("span", "prompt-label", row.label),
        );
        item.addEventListener("click", () => row.choose());
        return item;
      }),
    );
    status.textContent = view.status ?? "";
    status.hidden = view.status === null;
    more.hidden = !view.more;
  };
}

/**
 * A field over `list` that hides the rows whose label does not contain what
 * the user typed, ignoring case, and says so when none is left. Focus starts
 * here, so the user can type at once.
 */
function searchField(
  search: NonNullable<PromptRequest<unknown>["search"]>,
  list: HTMLElement,
  onType: (text: string) => void,
): HTMLElement {
  const box = el("div", "prompt-search");
  const input = el("input", "");
  // A text field, not `type="search"`: a search field eats the first Escape
  // to clear itself, and Escape must dismiss the sheet as everywhere else.
  input.type = "text";
  input.inputMode = "search";
  input.enterKeyHint = "search";
  input.setAttribute("role", "searchbox");
  input.placeholder = search.placeholder;
  input.setAttribute("aria-label", search.label);
  input.autocomplete = "off";
  input.spellcheck = false;
  input.autocapitalize = "off";
  input.autofocus = true;
  const empty = el("p", "prompt-empty", search.empty);
  empty.hidden = true;
  empty.setAttribute("role", "status");
  input.addEventListener("input", () => {
    const query = input.value.trim().toLowerCase();
    let shown = 0;
    for (const row of list.querySelectorAll<HTMLElement>(".prompt-row")) {
      const label = row.querySelector(".prompt-label")?.textContent ?? "";
      const match = label.toLowerCase().includes(query);
      row.hidden = !match;
      if (match) shown += 1;
    }
    empty.hidden = shown > 0;
    onType(input.value);
  });
  box.append(input);
  if (search.lookup !== undefined)
    box.append(el("h3", "prompt-group", search.lookup.ownHeading));
  box.append(empty);
  return box;
}

function show<T>(request: PromptRequest<T>): Promise<T> {
  const dialog = el("dialog", "prompt");
  const scrim = el("div", "prompt-scrim");
  const sheet = el("section", "prompt-sheet");
  const titleId = `prompt-title-${Math.random().toString(36).slice(2)}`;
  dialog.setAttribute("aria-labelledby", titleId);

  const grabber = el("span", "prompt-grabber");
  grabber.setAttribute("aria-hidden", "true");
  const title = el("h2", "", request.title);
  title.id = titleId;
  title.tabIndex = -1;
  // Focus starts on the title, so Enter does not answer before the user reads,
  // or on the search field when there is one.
  title.autofocus = request.search === undefined;
  sheet.append(grabber, title);
  if (request.from !== undefined) {
    sheet.append(el("p", "prompt-from", `Requested by ${request.from}`));
    const known = describeRequester(request.from);
    if (known !== null) sheet.append(el("p", "prompt-from-known", known));
  }
  if (request.subtitle)
    sheet.append(el("p", "prompt-subtitle", request.subtitle));

  if (request.fields.length > 0) {
    const fields = el("dl", "prompt-fields");
    for (const field of request.fields) {
      const value = el(field.mono ? "pre" : "dd", "", field.value);
      if (field.warning) value.classList.add("warning");
      fields.append(el("dt", "", field.label), value);
    }
    sheet.append(fields);
  }

  return new Promise<T>((resolve) => {
    const before = document.activeElement;
    let stopFollowing = () => {};
    let lookupSession: ReturnType<PromptLookup<T>["open"]> | null = null;
    let answered = false;
    const finish = (value: T) => {
      if (answered) return;
      answered = true;
      lookupSession?.close();
      stopFollowing();
      dialog.close();
      dialog.remove();
      if (before instanceof HTMLElement && before.isConnected) before.focus();
      resolve(value);
    };

    const listed = request.layout === "list";
    const rows = listed
      ? request.choices.filter((choice) => choice.tone !== "cancel")
      : [];
    const footer = request.choices.filter((choice) => !rows.includes(choice));
    if (rows.length > 0 || request.search?.lookup !== undefined) {
      const list = el("div", "prompt-list");
      for (const choice of rows)
        list.append(choiceButton(choice, "prompt-row", finish));
      const lookup = request.search?.lookup;
      const group = el("section", "prompt-lookup");
      if (lookup !== undefined) {
        const redraw = lookupGroup(lookup, group, () => lookupSession?.more());
        redraw({
          visible: false,
          rows: [],
          enabled: true,
          status: null,
          more: false,
        });
        lookupSession = lookup.open(redraw, finish);
      }
      if (request.search !== undefined)
        sheet.append(
          searchField(request.search, list, (text) =>
            lookupSession?.type(text),
          ),
        );
      sheet.append(list);
      if (lookup !== undefined) sheet.append(group);
    }
    const actions = el("div", "prompt-actions");
    actions.dataset.layout = listed ? "stack" : "row";
    for (const choice of footer)
      actions.append(choiceButton(choice, "prompt-action", finish));
    sheet.append(actions);

    scrim.addEventListener("click", () => finish(request.dismissed));
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      finish(request.dismissed);
    });
    dialog.append(scrim, sheet);
    document.body.append(dialog);
    stopFollowing = followArea(dialog);
    dialog.showModal();
  });
}
