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
}

let queue: Promise<unknown> = Promise.resolve();
let area: HTMLElement | null = null;

/** Draw prompts over `element`, the app's rectangle, instead of the whole page. */
export function setPromptArea(element: HTMLElement | null): void {
  area = element;
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
  // Focus starts on the title, so Enter does not answer before the user reads.
  title.autofocus = true;
  sheet.append(grabber, title);
  if (request.from !== undefined)
    sheet.append(el("p", "prompt-from", `Requested by ${request.from}`));
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
    let answered = false;
    const finish = (value: T) => {
      if (answered) return;
      answered = true;
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
    if (rows.length > 0) {
      const list = el("div", "prompt-list");
      for (const choice of rows)
        list.append(choiceButton(choice, "prompt-row", finish));
      sheet.append(list);
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
