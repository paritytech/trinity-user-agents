import type { PromptField } from "./reviews.js";

/** One button of a prompt and the answer it gives. */
export interface PromptChoice<T> {
  label: string;
  value: T;
  primary?: boolean;
}

export interface PromptRequest<T> {
  title: string;
  subtitle?: string;
  fields: PromptField[];
  choices: PromptChoice<T>[];
  /** The answer when the prompt is dismissed without choosing, as with Escape. */
  dismissed: T;
}

let queue: Promise<unknown> = Promise.resolve();

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

function show<T>(request: PromptRequest<T>): Promise<T> {
  const dialog = document.createElement("dialog");
  dialog.className = "prompt";

  const title = document.createElement("h2");
  title.textContent = request.title;
  dialog.append(title);

  if (request.subtitle) {
    const subtitle = document.createElement("p");
    subtitle.className = "prompt-subtitle";
    subtitle.textContent = request.subtitle;
    dialog.append(subtitle);
  }

  const fields = document.createElement("dl");
  for (const field of request.fields) {
    const label = document.createElement("dt");
    label.textContent = field.label;
    const value = document.createElement(field.mono ? "pre" : "dd");
    value.textContent = field.value;
    if (field.warning) value.classList.add("warning");
    fields.append(label, value);
  }
  dialog.append(fields);

  const actions = document.createElement("div");
  actions.className = "prompt-actions";
  dialog.append(actions);
  document.body.append(dialog);

  return new Promise<T>((resolve) => {
    const finish = (value: T) => {
      dialog.close();
      dialog.remove();
      resolve(value);
    };
    for (const choice of request.choices) {
      const button = document.createElement("button");
      button.textContent = choice.label;
      if (choice.primary) button.classList.add("primary");
      button.addEventListener("click", () => finish(choice.value));
      actions.append(button);
    }
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      finish(request.dismissed);
    });
    dialog.showModal();
  });
}
