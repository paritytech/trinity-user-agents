import { bytesToHex } from "@parity/truapi/scale";
import type { HexString, ImageSource, RenderContext } from "@parity/truapi";
import type { TrUApiProductProvider } from "@parity/truapi-host";
import type {
  ProductChat,
  ProductMessage,
  ProductRoom,
} from "./product-chat.js";
import { bulletinImageUrl, renderTree } from "./product-render.js";

/**
 * One product room on the Chat screen: its messages, custom messages drawn by
 * the product's worker through the core, and the text input when the room
 * has one.
 *
 * The view owns its DOM and its render streams. Messages are drawn once and
 * kept, so redrawing the screen around them restarts no stream. Leaving the
 * room, or the worker going away, ends every stream it opened.
 */

export interface ProductRoomDeps {
  chat: ProductChat;
  /** The running worker's channel, or null when it is not running. */
  provider(productId: string): TrUApiProductProvider | null;
  archiveImage(productId: string, path: string): Promise<string | null>;
  gateway: string;
  log(text: string): void;
}

/** The peer a user's message and actions come from, as the native hosts name it. */
export const USER_PEER = "native";

const time = (at: number): string =>
  new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

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

interface Drawn {
  element: HTMLElement;
  /** Ends the message's render stream, if it has one. */
  stop(): void;
  /** The provider the stream was opened on; a new worker means a new stream. */
  provider: TrUApiProductProvider | null;
}

export class ProductRoomView {
  readonly thread = el("div", "chat-thread product-thread");
  readonly footer = el("div", "chat-footer");
  private readonly drawn = new Map<string, Drawn>();
  private draft = "";
  private disposed = false;

  constructor(
    readonly productId: string,
    readonly roomId: string,
    private readonly deps: ProductRoomDeps,
  ) {
    this.thread.setAttribute("role", "log");
    this.thread.dataset.productRoom = roomId;
  }

  private get room(): ProductRoom | undefined {
    return this.deps.chat.room(this.productId, this.roomId);
  }

  /** Bring the thread and the footer up to date with the store. */
  refresh(): void {
    if (this.disposed) return;
    const provider = this.deps.provider(this.productId);
    const messages = this.deps.chat.messages(this.productId, this.roomId);
    const keep = new Set(messages.map((message) => message.messageId));
    for (const [id, drawn] of this.drawn)
      if (!keep.has(id)) {
        drawn.stop();
        drawn.element.remove();
        this.drawn.delete(id);
      }
    const atEnd =
      this.thread.scrollHeight -
        this.thread.scrollTop -
        this.thread.clientHeight <
      32;
    for (const message of messages) {
      const known = this.drawn.get(message.messageId);
      if (known !== undefined && known.provider === provider) continue;
      known?.stop();
      const drawn = this.draw(message, provider);
      if (known !== undefined) known.element.replaceWith(drawn.element);
      else this.thread.append(drawn.element);
      this.drawn.set(message.messageId, drawn);
    }
    if (atEnd) this.thread.scrollTop = this.thread.scrollHeight;
    this.drawFooter(provider);
  }

  private draw(
    message: ProductMessage,
    provider: TrUApiProductProvider | null,
  ): Drawn {
    const row = el("div", `chat-message chat-message-${message.direction}`);
    row.dataset.messageId = message.messageId;
    const content = message.content;
    if (content.tag === "Text") {
      const bubble = el("div", "chat-bubble");
      bubble.append(
        el("span", "chat-bubble-text", content.text),
        el("span", "chat-bubble-meta", time(message.at)),
      );
      row.append(bubble);
      return { element: row, stop: () => {}, provider };
    }
    row.classList.add("product-card-row");
    const card = el("div", "product-card");
    card.dataset.messageType = content.messageType;
    row.append(card);
    const fallback = (reason: string) => {
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
    if (provider?.render === undefined) {
      fallback("The product's worker is not running.");
      return { element: row, stop: () => {}, provider };
    }
    const context: RenderContext = {
      tag: "ChatMessage",
      value: {
        roomId: message.roomId,
        messageId: message.messageId,
        messageType: content.messageType,
      },
    };
    card.dataset.state = "loading";
    card.append(el("span", "product-card-note", "Drawing…"));
    const stop = provider.render(
      { context, payload: content.payload },
      {
        onUpdate: (tree) => {
          try {
            card.replaceChildren(
              renderTree(tree, {
                action: (actionId, payload) => {
                  void provider
                    .publishRendererAction?.({
                      context,
                      actionId,
                      payload: bytesToHex(payload) as HexString,
                    })
                    .catch((error: unknown) =>
                      this.deps.log(
                        `${this.productId}: action ${actionId} not delivered: ${errorText(error)}`,
                      ),
                    );
                },
                image: (source) => this.image(source),
                unsupported: (what) =>
                  this.deps.log(
                    `${this.productId}: this host does not draw ${what}`,
                  ),
              }),
            );
            card.dataset.state = "drawn";
          } catch (error) {
            fallback(`Could not draw this message: ${errorText(error)}`);
          }
        },
        onError: (error) =>
          fallback(`Could not draw this message: ${errorText(error)}`),
      },
    );
    return { element: row, stop, provider };
  }

  private image(source: ImageSource): Promise<string | null> | string | null {
    return source.tag === "Archive"
      ? this.deps.archiveImage(this.productId, source.value)
      : bulletinImageUrl(source.value, this.deps.gateway);
  }

  private drawFooter(provider: TrUApiProductProvider | null): void {
    const room = this.room;
    if (room === undefined || room.footer === "Empty") {
      this.footer.hidden = true;
      this.footer.replaceChildren();
      return;
    }
    this.footer.hidden = false;
    const focused = document.activeElement;
    const hadFocus =
      focused instanceof HTMLElement && this.footer.contains(focused);
    const form = el("form", "chat-composer");
    const input = el("textarea", "chat-input chat-composer-input");
    input.rows = 1;
    input.placeholder =
      provider === null ? "The product's worker is not running" : "Message";
    input.value = this.draft;
    input.disabled = provider === null;
    input.addEventListener("input", () => (this.draft = input.value));
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        form.requestSubmit();
      }
    });
    const send = el("button", "chat-send", "Send");
    send.type = "submit";
    send.disabled = provider === null;
    form.append(input, send);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const text = this.draft.trim();
      if (text === "" || provider?.publishChatAction === undefined) return;
      this.draft = "";
      this.deps.chat.addOutgoing(this.productId, this.roomId, text);
      void provider
        .publishChatAction({
          roomId: this.roomId,
          peer: USER_PEER,
          payload: {
            tag: "MessagePosted",
            value: { tag: "Text", value: { text } },
          },
        })
        .catch((error: unknown) =>
          this.deps.log(
            `${this.productId}: message not delivered: ${errorText(error)}`,
          ),
        );
    });
    this.footer.replaceChildren(form);
    if (hadFocus) input.focus();
  }

  /** End every render stream and drop the drawn messages. */
  dispose(): void {
    this.disposed = true;
    for (const drawn of this.drawn.values()) drawn.stop();
    this.drawn.clear();
    this.thread.replaceChildren();
    this.footer.replaceChildren();
  }
}
