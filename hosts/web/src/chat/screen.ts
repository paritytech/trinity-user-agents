import type { ChatInboxView, InboxChange } from "./inbox.js";
import type {
  ChatContact,
  ChatProvider,
  ConversationSummary,
} from "./provider.js";
import { CallOverlay } from "./call-view.js";
import type { PickedPerson } from "../contacts/contacts.js";
import { ConversationView, avatar, time } from "./conversation-view.js";

/**
 * The host's own Chat screen, laid out like the Polkadot app's: a large
 * "Chats" title, one list of conversations, and a conversation with bubbles
 * and a composer. It is drawn by the host, not by
 * a product, and is not offered through TrUAPI.
 *
 * The screen knows chats only as chat providers (`src/chat/provider.ts`):
 * every provider's conversations share one list, ordered by their last
 * activity, and open in the same conversation view. Starting a chat shows
 * only when a provider offers it. Chat keeps running while the screen is hidden.
 */

type View =
  | { kind: "list" }
  | { kind: "request"; providerId: string; person: PickedPerson }
  | { kind: "conversation"; providerId: string; conversationId: string }
  | { kind: "blocked" };

/** The people the user blocked, as the host keeps them, and blocking or unblocking one. */
export interface BlockedPeople {
  list(): readonly ChatContact[];
  /** The host's contacts the user may block now. */
  blockable(): readonly ChatContact[];
  /** The host's contact picker over the contacts the user may block, or null when closed. */
  choose(): Promise<PickedPerson | null>;
  block(account: string): void;
  unblock(account: string): void;
  onChange(listener: () => void): () => void;
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

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class ChatScreen {
  private view: View = { kind: "list" };
  private conversation: ConversationView | null = null;
  private newWelcome = "";
  /** Whether New chat's picker is open, so a second tap does not queue another. */
  private choosing = false;
  private formError: string | null = null;
  private busy = false;
  private visible = false;
  private renderQueued = false;
  /** The call layer, over every screen, so an incoming call shows wherever the user is. */
  private readonly call = new CallOverlay();
  /** What shows in place of the signed-out note while the tab's session runs no chat, or null. */
  private noChat: { title: string; body: string } | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly inbox: ChatInboxView,
    /**
     * The host's contact picker, which New chat opens. Absent, New chat is
     * not offered.
     */
    private readonly choosePerson?: () => Promise<PickedPerson | null>,
    /** Drawn under the title of the list, such as the tabs to the Inbox. */
    private readonly listHeader?: HTMLElement,
    /** The people the user blocked. Absent, no Blocked list or Block is offered. */
    private readonly blocked?: BlockedPeople,
  ) {
    document.body.append(this.call.root);
    inbox.onChange((change) => this.follow(change));
    blocked?.onChange(() => this.queueRender());
  }

  private follow(change: InboxChange): void {
    if (change.type === "session") {
      this.leaveConversation();
      this.view = { kind: "list" };
      this.newWelcome = "";
      this.formError = null;
    }
    if (
      change.type === "provider" &&
      this.conversation?.provider.id === change.providerId
    )
      this.conversation.onEvent(change.event);
    const providers = this.inbox.providers();
    this.call.follow(
      providers.flatMap((provider) =>
        provider.calls === undefined ? [] : [provider.calls],
      ),
    );
    this.queueRender();
  }

  /**
   * Say why there is no chat while the tab is signed in to a session that
   * runs none, in place of the note that asks the user to sign in.
   */
  setNoChat(note: { title: string; body: string } | null): void {
    this.noChat = note;
    this.render();
  }

  /** Show or hide the screen. Chat keeps running while it is hidden. */
  show(visible: boolean): void {
    this.visible = visible;
    this.root.hidden = !visible;
    this.render();
    if (visible) this.conversation?.reveal();
  }

  private queueRender(): void {
    if (this.renderQueued) return;
    this.renderQueued = true;
    queueMicrotask(() => {
      this.renderQueued = false;
      this.render();
    });
  }

  private provider(id: string): ChatProvider | undefined {
    return this.inbox.providers().find((provider) => provider.id === id);
  }

  private open(providerId: string, conversationId: string): void {
    this.leaveConversation();
    this.view = { kind: "conversation", providerId, conversationId };
    this.render();
  }

  private toList(): void {
    this.leaveConversation();
    this.view = { kind: "list" };
    this.formError = null;
    this.render();
  }

  private leaveConversation(): void {
    this.conversation?.dispose();
    this.conversation = null;
  }

  private act(work: () => Promise<void>): void {
    this.busy = true;
    this.formError = null;
    this.render();
    work()
      .catch((error: unknown) => (this.formError = errorText(error)))
      .finally(() => {
        this.busy = false;
        this.render();
      });
  }

  // -------------------------------------------------------------------------
  // Drawing
  // -------------------------------------------------------------------------

  render(): void {
    if (!this.visible) return;
    if (this.view.kind === "conversation" && this.drawConversation()) return;
    const focused = document.activeElement;
    const focusKey =
      focused instanceof HTMLElement && this.root.contains(focused)
        ? focused.dataset.focus
        : undefined;
    const selection =
      focused instanceof HTMLTextAreaElement ||
      focused instanceof HTMLInputElement
        ? [focused.selectionStart, focused.selectionEnd]
        : null;

    this.root.replaceChildren(...this.draw());

    if (focusKey !== undefined) {
      const again = this.root.querySelector<HTMLElement>(
        `[data-focus="${focusKey}"]`,
      );
      again?.focus();
      if (
        selection &&
        (again instanceof HTMLTextAreaElement ||
          again instanceof HTMLInputElement)
      )
        again.setSelectionRange(selection[0], selection[1]);
    }
  }

  /**
   * Show the open conversation. Its elements are put on the screen once and
   * then kept, so a redraw never restarts its render streams or moves the
   * reader. False when it cannot be shown and the list is drawn instead.
   */
  private drawConversation(): boolean {
    if (this.view.kind !== "conversation") return false;
    const { providerId, conversationId } = this.view;
    const provider = this.provider(providerId);
    if (
      this.conversation !== null &&
      (this.conversation.provider !== provider ||
        this.conversation.conversationId !== conversationId)
    )
      this.leaveConversation();
    if (provider === undefined) {
      this.view = { kind: "list" };
      return false;
    }
    if (this.conversation === null) {
      const blocked = this.blocked;
      this.conversation = new ConversationView(
        provider,
        conversationId,
        () => this.toList(),
        blocked === undefined
          ? undefined
          : {
              // The host's contact whose conversation this is, as the provider finds it by account.
              person: () =>
                blocked
                  .blockable()
                  .find(
                    (contact) =>
                      provider.start?.conversationWith?.(contact.account) ===
                      conversationId,
                  ) ?? null,
              block: (account) => blocked.block(account),
            },
      );
      if (!this.conversation.exists()) {
        this.leaveConversation();
        this.view = { kind: "list" };
        return false;
      }
      this.root.replaceChildren(
        this.conversation.bar,
        this.conversation.thread,
        this.conversation.footer,
      );
      return true;
    }
    if (!this.conversation.exists()) {
      this.toList();
      return true;
    }
    if (this.conversation.bar.parentElement !== this.root)
      this.root.replaceChildren(
        this.conversation.bar,
        this.conversation.thread,
        this.conversation.footer,
      );
    this.conversation.update();
    return true;
  }

  private draw(): HTMLElement[] {
    if (!this.inbox.signedIn())
      return [
        this.bar("Chats"),
        ...this.header(),
        this.noChat === null
          ? this.empty(
              "Sign in to chat",
              "Choose a wallet in Settings. Chat uses that wallet's identity.",
            )
          : this.empty(this.noChat.title, this.noChat.body),
      ];
    if (this.view.kind === "blocked") return this.drawBlocked();
    if (this.view.kind === "request") {
      const provider = this.provider(this.view.providerId);
      if (provider?.start !== undefined)
        return this.drawRequestForm(provider, this.view.person);
      this.view = { kind: "list" };
    }
    return this.drawList();
  }

  private bar(title: string, action?: HTMLElement): HTMLElement {
    const bar = el("header", "chat-bar");
    bar.append(el("h2", "chat-title", title));
    if (action) bar.append(action);
    return bar;
  }

  private back(): HTMLButtonElement {
    const back = button("chat-icon-button", "‹", () => this.toList());
    back.setAttribute("aria-label", "Back to chats");
    return back;
  }

  private empty(title: string, body: string): HTMLElement {
    const node = el("div", "chat-empty");
    node.append(
      el("p", "chat-empty-title", title),
      el("p", "chat-empty-body", body),
    );
    return node;
  }

  private drawList(): HTMLElement[] {
    const providers = this.inbox.providers();
    const starter =
      this.choosePerson === undefined ? undefined : this.starter();
    const body = el("div", "chat-body chat-inbox");
    const rows = providers
      .flatMap((provider) =>
        provider.conversations().map((summary) => ({ provider, summary })),
      )
      .sort((left, right) => right.summary.updatedAt - left.summary.updatedAt);
    if (rows.length > 0) {
      const list = el("ul", "chat-list");
      for (const { provider, summary } of rows)
        list.append(this.drawRow(provider, summary));
      body.append(list);
    } else
      body.append(
        this.empty(
          "Add your first contact",
          starter === undefined
            ? "Add a product's worker in Settings to chat with it."
            : "Tap + to choose someone to chat with.",
        ),
      );
    const blocked = this.blocked?.list().length ?? 0;
    // Reachable while there is someone to unblock or to block.
    if (blocked > 0 || (this.blocked?.blockable().length ?? 0) > 0) {
      const links = el("p", "chat-inbox-links");
      const label = blocked > 0 ? `Blocked (${blocked})` : "Blocked";
      const open = button("chat-link", label, () => {
        this.view = { kind: "blocked" };
        this.formError = null;
        this.render();
      });
      open.dataset.action = "blocked";
      links.append(open);
      body.append(links);
    }
    if (this.formError !== null)
      body.append(el("p", "chat-error", this.formError));

    let add: HTMLButtonElement | undefined;
    if (starter !== undefined) {
      add = button("chat-icon-button", "+", () => void this.newChat(starter));
      add.setAttribute("aria-label", "New chat");
      add.title = "New chat";
    }
    return [this.bar("Chats", add), ...this.header(), body];
  }

  private header(): HTMLElement[] {
    return this.listHeader === undefined ? [] : [this.listHeader];
  }

  private drawRow(
    provider: ChatProvider,
    summary: ConversationSummary,
  ): HTMLElement {
    const item = el("li", "");
    const row = button("chat-row chat-row-button", "", () =>
      this.open(provider.id, summary.id),
    );
    row.dataset.conversation = `${provider.id}/${summary.id}`;
    row.dataset.provider = provider.id;
    row.dataset.updatedAt = String(summary.updatedAt);
    const text = el("span", "chat-row-text");
    text.append(
      el("span", "chat-row-title", summary.title),
      el("span", "chat-row-preview", summary.preview),
    );
    const side = el("span", "chat-row-side");
    const state = provider.status().state;
    side.append(
      el(
        "span",
        "chat-row-time",
        state === "offline" || state === "failed"
          ? "offline"
          : summary.updatedAt > 0
            ? time(summary.updatedAt)
            : "",
      ),
    );
    if (summary.unread !== undefined && summary.unread > 0) {
      const badge = el("span", "chat-unread", String(summary.unread));
      badge.setAttribute("aria-label", `${summary.unread} unread`);
      side.append(badge);
    }
    row.append(avatar(summary), text, side);
    item.append(row);
    return item;
  }

  /**
   * New chat: the host's contact picker, the same one products open. A person
   * the user already chats with opens that conversation. Anyone else opens a
   * request to them, which is sent only when the user asks. Closing the
   * picker leaves the screen as it was.
   */
  private async newChat(provider: ChatProvider): Promise<void> {
    if (this.choosing) return;
    this.choosing = true;
    this.formError = null;
    const person = await this.choosePerson!().finally(
      () => (this.choosing = false),
    );
    if (person === null || this.provider(provider.id) !== provider) return;
    this.showPerson(provider, person);
  }

  /** The provider that starts conversations, as New chat does, or none. */
  private starter(): ChatProvider | undefined {
    return this.inbox
      .providers()
      .find((provider) => provider.start !== undefined);
  }

  /** Whether a person chosen elsewhere, such as in the Docklet's search, can be shown here. */
  canStart(): boolean {
    return this.inbox.signedIn() && this.starter() !== undefined;
  }

  /** Why a person chosen elsewhere cannot be shown here, or null when they can. */
  startProblem(): string | null {
    return this.canStart()
      ? null
      : "Chat cannot start a conversation in this session.";
  }

  /**
   * Show `person`, chosen outside Chat, as New chat would: their
   * conversation, or a request to them that is sent only when the user asks.
   * False when no provider here can start one.
   */
  startWith(person: PickedPerson): boolean {
    const provider = this.canStart() ? this.starter() : undefined;
    if (provider === undefined) return false;
    this.formError = null;
    this.showPerson(provider, person);
    this.root
      .querySelector<HTMLElement>(
        '[data-focus="welcome"], [data-focus="composer"]',
      )
      ?.focus();
    return true;
  }

  private showPerson(provider: ChatProvider, person: PickedPerson): void {
    const existing =
      provider.start?.conversationWith !== undefined
        ? provider.start.conversationWith(person.account)
        : (provider
            .conversations()
            .find(
              (summary) =>
                summary.id.toLowerCase() === person.account.toLowerCase(),
            )?.id ?? null);
    if (existing !== null) return this.open(provider.id, existing);
    this.leaveConversation();
    this.newWelcome = "";
    this.view = { kind: "request", providerId: provider.id, person };
    this.render();
  }

  /** A chat request to `person`, with an optional first message, sent through the provider's own start. */
  private drawRequestForm(
    provider: ChatProvider,
    person: PickedPerson,
  ): HTMLElement[] {
    const start = provider.start!;
    const body = el("div", "chat-body chat-new");
    const form = el("form", "chat-form chat-new-request");
    const welcome = el("textarea", "chat-input chat-textarea");
    welcome.dataset.focus = "welcome";
    welcome.placeholder = "First message";
    welcome.value = this.newWelcome;
    welcome.setAttribute("aria-label", "First message");
    welcome.addEventListener("input", () => (this.newWelcome = welcome.value));
    const submit = el(
      "button",
      "chat-pill chat-pill-primary",
      `Send request to ${person.name}`,
    );
    submit.type = "submit";
    submit.dataset.action = "start-chat";
    const blocked = start.blocked();
    submit.disabled = this.busy || blocked !== null;
    form.append(
      el(
        "p",
        "chat-identity-line",
        `${person.name} sees your request and decides whether to chat.`,
      ),
      welcome,
      submit,
    );
    if (blocked !== null) form.append(el("p", "chat-note", blocked));
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      this.act(async () => {
        const conversationId = await start.start(
          person.account,
          this.newWelcome.trim(),
        );
        this.newWelcome = "";
        this.open(provider.id, conversationId);
      });
    });
    body.append(form);
    if (this.formError !== null)
      body.append(el("p", "chat-error", this.formError));

    const bar = el("header", "chat-bar chat-bar-small");
    bar.append(
      this.back(),
      el("h2", "chat-title chat-title-small", "New chat"),
    );
    return [bar, body];
  }

  /** Choose one of the host's contacts in its picker, and block them. */
  private async blockContact(): Promise<void> {
    if (this.choosing || this.blocked === undefined) return;
    this.choosing = true;
    const person = await this.blocked
      .choose()
      .finally(() => (this.choosing = false));
    if (person !== null) this.blocked.block(person.account);
  }

  /** The people the user blocked, each with Unblock, and Block a contact. */
  private drawBlocked(): HTMLElement[] {
    const bar = el("header", "chat-bar chat-bar-small");
    bar.append(this.back(), el("h2", "chat-title chat-title-small", "Blocked"));
    const body = el("div", "chat-body");
    const add = button(
      "chat-pill",
      "Block a contact",
      () => void this.blockContact(),
    );
    add.dataset.action = "block-contact";
    add.disabled = (this.blocked?.blockable().length ?? 0) === 0;
    body.append(add);
    const list = el("ul", "chat-list chat-blocked");
    for (const person of this.blocked?.list() ?? []) {
      const item = el("li", "chat-row");
      item.dataset.account = person.account;
      const text = el("span", "chat-row-text");
      text.append(el("span", "chat-row-title", person.name));
      const unblock = button("chat-pill", "Unblock", () =>
        this.blocked!.unblock(person.account),
      );
      unblock.dataset.action = "unblock";
      item.append(
        avatar({ icon: null, seed: person.account, title: person.name }),
        text,
        unblock,
      );
      list.append(item);
    }
    if (list.childElementCount === 0)
      body.append(
        this.empty("No one is blocked", "People you block show here."),
      );
    else body.append(list);
    body.append(
      el("p", "chat-identity-line", "Blocking is kept on this device only."),
    );
    return [bar, body];
  }
}
