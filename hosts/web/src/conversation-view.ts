import type {
  ChatMessage,
  ChatProvider,
  ConversationSummary,
  ProviderEvent,
} from "./chat-provider.js";
import { renderTree } from "./product-render.js";

/**
 * One conversation on the Chat screen, for any provider: a header, the
 * messages, and what the provider puts below them.
 *
 * The view owns its DOM. Messages are kept by id and drawn once, so an update
 * replaces one row and an open `Custom` render keeps running. History is read
 * a page at a time, newest first, and an earlier page is read when the user
 * scrolls to the top. The view tells the provider which message the user has
 * actually seen, and only once it is on screen.
 *
 * Every read carries the view's token and the provider's epoch, and a reply
 * that comes back under another is dropped: leaving the conversation, a new
 * session, or a restarted source all end what was asked before.
 */

interface Row {
  message: ChatMessage;
  element: HTMLElement;
  /** Ends the message's render stream, if it has one. */
  stop(): void;
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

function button(
  className: string,
  label: string,
  onClick: () => void,
): HTMLButtonElement {
  const node = el("button", className, label);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

const AVATAR_COLORS = [
  "#e6007a",
  "#56f39a",
  "#ffd37f",
  "#7f9dff",
  "#ff8f6b",
  "#b98cff",
];

/** A conversation's icon when it has one, else its initial in a color its seed picks. */
export function avatar(
  summary: Pick<ConversationSummary, "icon" | "seed" | "title">,
  size: "large" | "small" = "large",
): HTMLElement {
  if (summary.icon !== null) {
    const image = el("img", `chat-avatar chat-avatar-${size} product-avatar`);
    image.src = summary.icon;
    image.alt = "";
    image.referrerPolicy = "no-referrer";
    return image;
  }
  const node = el(
    "span",
    `chat-avatar chat-avatar-${size}`,
    (summary.title || "?").slice(0, 1).toUpperCase(),
  );
  const hue =
    [...summary.seed].reduce((sum, char) => sum + char.charCodeAt(0), 0) %
    AVATAR_COLORS.length;
  node.style.setProperty("--avatar", AVATAR_COLORS[hue]);
  node.setAttribute("aria-hidden", "true");
  return node;
}

export const time = (at: number): string =>
  new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const STATUS_MARK: Record<ChatMessage["status"], string> = {
  pending: " · sending",
  sent: " · sent",
  delivered: " · delivered",
  read: " · read",
  failed: " · failed",
  received: "",
};

/** How close to the end, in pixels, still counts as reading the newest messages. */
const AT_END = 32;
/** How close to the top, in pixels, asks for the page before. */
const NEAR_TOP = 48;

export class ConversationView {
  readonly bar = el("header", "chat-bar chat-bar-small");
  readonly thread = el("div", "chat-thread");
  readonly footer = el("div", "chat-footer");
  private readonly earlier = button(
    "chat-link chat-earlier",
    "Earlier messages",
    () => void this.loadEarlier(),
  );
  private readonly rows = new Map<string, Row>();
  /** Message ids, oldest first, as drawn. */
  private order: string[] = [];
  /** The cursor for the page before the oldest drawn message, if there is one. */
  private before: string | undefined;
  private loaded = false;
  private loadingEarlier = false;
  private token = 0;
  private epoch: number;
  private seenThrough: string | null = null;
  /** Whether the reader is at the newest messages, as of the last time the thread was on screen. */
  private atEnd = true;
  private draft = "";
  private error: string | null = null;
  private composerShown = "";
  private readonly input = el("textarea", "chat-input chat-composer-input");
  private readonly onVisibility = () => this.reportSeen();

  constructor(
    readonly provider: ChatProvider,
    readonly conversationId: string,
    private readonly onBack: () => void,
  ) {
    this.epoch = provider.epoch();
    this.thread.setAttribute("role", "log");
    this.thread.dataset.conversation = `${provider.id}/${conversationId}`;
    this.thread.addEventListener("scroll", () => {
      this.atEnd = this.readerAtEnd();
      if (this.thread.scrollTop < NEAR_TOP) void this.loadEarlier();
      this.reportSeen();
    });
    document.addEventListener("visibilitychange", this.onVisibility);
    this.input.rows = 1;
    this.input.dataset.focus = "composer";
    this.input.addEventListener("input", () => (this.draft = this.input.value));
    this.update();
    void this.load();
  }

  private get summary(): ConversationSummary | undefined {
    return this.provider
      .conversations()
      .find((entry) => entry.id === this.conversationId);
  }

  /** Whether the conversation still exists. */
  exists(): boolean {
    return this.summary !== undefined;
  }

  /** Bring the header and the footer up to date, and start over if the provider's epoch moved. */
  update(): void {
    if (this.provider.epoch() !== this.epoch) {
      this.epoch = this.provider.epoch();
      this.clear();
      void this.load();
    }
    this.drawBar();
    this.drawFooter();
  }

  /** Follow one of the provider's events. */
  onEvent(event: ProviderEvent): void {
    if (
      event.type === "messages" &&
      event.conversationId === this.conversationId
    ) {
      if (event.message !== undefined) this.place([event.message], "keep");
      else void this.load();
    }
    this.update();
  }

  /** Read the newest page and merge it with what is drawn. */
  private async load(): Promise<void> {
    const token = this.token;
    const epoch = this.epoch;
    try {
      const page = await this.provider.timeline(this.conversationId);
      if (token !== this.token || epoch !== this.provider.epoch()) return;
      if (!this.loaded) this.before = page.before;
      this.loaded = true;
      const held = new Set(page.messages.map((message) => message.id));
      const first = page.messages[0]?.at;
      const last = page.messages.at(-1)?.at;
      // A message inside the page's span that the page no longer holds is gone.
      if (first !== undefined && last !== undefined)
        for (const id of [...this.order]) {
          const at = this.rows.get(id)!.message.at;
          if (!held.has(id) && at >= first && at <= last) this.drop(id);
        }
      this.place(page.messages, "end");
    } catch (error) {
      if (token !== this.token) return;
      this.error = `Could not read the conversation: ${errorText(error)}`;
      this.drawFooter();
    }
  }

  /** Read the page before the oldest drawn message, keeping the reader where they are. */
  private async loadEarlier(): Promise<void> {
    if (!this.loaded || this.before === undefined || this.loadingEarlier)
      return;
    const token = this.token;
    const epoch = this.epoch;
    this.loadingEarlier = true;
    try {
      const page = await this.provider.timeline(
        this.conversationId,
        this.before,
      );
      if (token !== this.token || epoch !== this.provider.epoch()) return;
      this.before = page.before;
      const height = this.thread.scrollHeight;
      this.place(page.messages, "start");
      this.thread.scrollTop += this.thread.scrollHeight - height;
    } catch (error) {
      if (token === this.token)
        this.error = `Could not read earlier messages: ${errorText(error)}`;
    } finally {
      if (token === this.token) this.loadingEarlier = false;
      this.drawFooter();
    }
  }

  /**
   * Draw `messages`, replacing a row whose message changed and inserting new
   * ones by time. `end` scrolls to the newest when the reader was there.
   */
  private place(
    messages: ChatMessage[],
    scroll: "end" | "start" | "keep",
  ): void {
    const atEnd = this.readerAtEnd();
    for (const message of messages) {
      const known = this.rows.get(message.id);
      if (known !== undefined) {
        if (JSON.stringify(known.message) === JSON.stringify(message)) continue;
        const keepsCard =
          message.content.tag === "Custom" &&
          JSON.stringify(known.message.content) ===
            JSON.stringify(message.content);
        if (keepsCard) {
          known.message = message;
          continue;
        }
        known.stop();
        const row = this.draw(message);
        known.element.replaceWith(row.element);
        this.rows.set(message.id, row);
        continue;
      }
      const row = this.draw(message);
      this.rows.set(message.id, row);
      const index = this.order.findIndex(
        (id) => this.rows.get(id)!.message.at > message.at,
      );
      if (index < 0) {
        this.thread.append(row.element);
        this.order.push(message.id);
      } else {
        this.thread.insertBefore(
          row.element,
          this.rows.get(this.order[index])!.element,
        );
        this.order.splice(index, 0, message.id);
      }
    }
    this.earlier.hidden = this.before === undefined;
    if (this.before !== undefined) this.thread.prepend(this.earlier);
    else this.earlier.remove();
    const first = scroll === "end" && this.order.length === messages.length;
    if (scroll !== "start" && (atEnd || first)) {
      this.thread.scrollTop = this.thread.scrollHeight;
      this.atEnd = true;
    }
    this.reportSeen();
  }

  /** Whether the reader is at the newest messages. A hidden thread keeps what it was. */
  private readerAtEnd(): boolean {
    if (this.thread.clientHeight === 0) return this.atEnd;
    return (
      this.thread.scrollHeight -
        this.thread.scrollTop -
        this.thread.clientHeight <
      AT_END
    );
  }

  /** The screen is shown again: follow the newest messages if the reader was there, and report what is now seen. */
  reveal(): void {
    if (this.atEnd) this.thread.scrollTop = this.thread.scrollHeight;
    this.reportSeen();
  }

  private drop(id: string): void {
    const row = this.rows.get(id);
    row?.stop();
    row?.element.remove();
    this.rows.delete(id);
    this.order = this.order.filter((entry) => entry !== id);
  }

  private clear(): void {
    this.token += 1;
    for (const id of [...this.order]) this.drop(id);
    this.before = undefined;
    this.loaded = false;
    this.loadingEarlier = false;
    this.seenThrough = null;
    this.earlier.remove();
  }

  /**
   * Tell the provider the newest message whose end is on screen, when the
   * page is visible. Nothing is reported for messages scrolled out of view or
   * for a hidden screen.
   */
  reportSeen(): void {
    if (this.provider.markRead === undefined) return;
    if (document.visibilityState !== "visible") return;
    if (!this.thread.isConnected || this.thread.clientHeight === 0) return;
    const view = this.thread.getBoundingClientRect();
    let seen: string | null = null;
    for (let index = this.order.length - 1; index >= 0; index -= 1) {
      const box = this.rows
        .get(this.order[index])!
        .element.getBoundingClientRect();
      if (box.bottom <= view.bottom + 1 && box.bottom > view.top) {
        seen = this.order[index];
        break;
      }
    }
    if (seen === null || seen === this.seenThrough) return;
    if (
      this.seenThrough !== null &&
      this.order.indexOf(seen) <= this.order.indexOf(this.seenThrough)
    )
      return;
    this.seenThrough = seen;
    this.provider.markRead(this.conversationId, seen);
  }

  private drawBar(): void {
    const summary = this.summary;
    const back = button("chat-icon-button", "‹", this.onBack);
    back.setAttribute("aria-label", "Back to chats");
    const heading = el("div", "chat-row-text");
    heading.append(
      el("span", "chat-row-title", summary?.title ?? ""),
      el("span", "chat-row-preview", summary?.subtitle ?? this.provider.label),
    );
    this.bar.replaceChildren(
      back,
      ...(summary === undefined ? [] : [avatar(summary, "small")]),
      heading,
    );
  }

  private drawFooter(): void {
    const composer = this.provider.composer(this.conversationId);
    const shown = `${JSON.stringify(composer)}|${this.error ?? ""}`;
    if (shown === this.composerShown) return;
    this.composerShown = shown;
    const hadFocus = document.activeElement === this.input;
    const parts: HTMLElement[] = [];
    if (composer.kind === "blocked")
      parts.push(el("p", "chat-waiting", composer.reason));
    if (composer.kind === "text") {
      const form = el("form", "chat-composer");
      this.input.placeholder = "Message";
      this.input.value = this.draft;
      this.input.onkeydown = (event) => {
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          form.requestSubmit();
        }
      };
      const send = el("button", "chat-send", "Send");
      send.type = "submit";
      form.append(this.input, send);
      form.addEventListener("submit", (event) => {
        event.preventDefault();
        const text = this.draft.trim();
        if (text === "") return;
        this.draft = this.input.value = "";
        this.error = null;
        this.drawFooter();
        this.provider
          .send(this.conversationId, text)
          .catch((error: unknown) => {
            this.error = errorText(error);
            this.drawFooter();
          });
      });
      parts.push(form);
    }
    if (this.error !== null) parts.push(el("p", "chat-error", this.error));
    this.footer.hidden = parts.length === 0;
    this.footer.replaceChildren(...parts);
    if (hadFocus) this.input.focus();
  }

  private retry(message: ChatMessage): HTMLButtonElement {
    return button("chat-link", "Retry", () => {
      this.error = null;
      this.provider
        .retry?.(this.conversationId, message.id)
        .catch((error: unknown) => {
          this.error = errorText(error);
          this.drawFooter();
        });
    });
  }

  private draw(message: ChatMessage): Row {
    const content = message.content;
    const canRetry =
      message.status === "failed" && this.provider.retry !== undefined;
    if (content.tag === "System") {
      const node = el("p", "chat-system", content.text);
      node.dataset.messageId = message.id;
      if (message.problem !== undefined) node.title = message.problem;
      if (canRetry) node.append(" ", this.retry(message));
      return { message, element: node, stop: () => {} };
    }
    const row = el(
      "div",
      `chat-message chat-message-${message.sender.self ? "out" : "in"}`,
    );
    row.dataset.messageId = message.id;
    row.dataset.status = message.status;
    row.dataset.sender = message.sender.id;
    let stop = () => {};
    if (content.tag === "Custom") stop = this.drawCustom(message, row);
    else row.append(this.bubble(message));
    if (message.status === "failed") {
      const failed = el(
        "p",
        "chat-failed",
        `Not sent: ${message.problem ?? "unknown reason"} `,
      );
      if (canRetry) failed.append(this.retry(message));
      row.append(failed);
    }
    return { message, element: row, stop };
  }

  /** A message's bubble: who sent it, what it says, and when. Every field is drawn as text. */
  private bubble(message: ChatMessage): HTMLElement {
    const content = message.content;
    const bubble = el("div", "chat-bubble");
    if (content.tag === "Text" && content.label !== undefined)
      bubble.append(el("span", "chat-bubble-label", content.label));
    if (!message.sender.self && message.sender.label !== this.summary?.title)
      bubble.append(el("span", "chat-bubble-sender", message.sender.label));
    if (content.tag === "Text")
      bubble.append(el("span", "chat-bubble-text", content.text));
    const meta = el("span", "chat-bubble-meta", time(message.at));
    if (message.sender.self)
      meta.append(el("span", "chat-tick", STATUS_MARK[message.status]));
    bubble.append(meta);
    return bubble;
  }

  /**
   * A `Custom` message, drawn by the provider as a tree, or its alt text when
   * it is not. Returns what stops the drawing.
   */
  private drawCustom(message: ChatMessage, row: HTMLElement): () => void {
    const content = message.content;
    if (content.tag !== "Custom") throw new Error("not a custom message");
    row.classList.add("product-card-row");
    if (!message.sender.self && message.sender.label !== this.summary?.title)
      row.append(el("span", "chat-bubble-sender", message.sender.label));
    const card = el("div", "product-card");
    card.dataset.messageType = content.messageType;
    row.append(card);
    const fail = (reason: string) => {
      card.dataset.state = "fallback";
      card.replaceChildren(
        el(
          "span",
          "product-card-alt",
          content.alt ?? `A ${content.messageType} message`,
        ),
        el("span", "product-card-note", reason),
      );
    };
    if (this.provider.renderCustom === undefined) {
      fail("This chat does not draw custom messages.");
      return () => {};
    }
    card.dataset.state = "loading";
    card.append(el("span", "product-card-note", "Drawing…"));
    const stop = this.provider.renderCustom(this.conversationId, message, {
      draw: (tree, hooks) => {
        try {
          card.replaceChildren(renderTree(tree, hooks));
          card.dataset.state = "drawn";
        } catch (error) {
          fail(`Could not draw this message: ${errorText(error)}`);
        }
      },
      fail,
    });
    return stop;
  }

  /** End every render stream and every read in flight. */
  dispose(): void {
    this.clear();
    document.removeEventListener("visibilitychange", this.onVisibility);
    this.thread.replaceChildren();
    this.footer.replaceChildren();
  }
}
