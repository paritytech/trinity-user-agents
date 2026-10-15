import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import type { ChatDiagnostics } from "./chat/diagnostics.js";
import { chainDirectory } from "./chat/directory.js";
import { ChatEngine, type ChatSnapshot } from "./chat/engine.js";
import { chatRefusal } from "./chat/gate.js";
import type { ProductChat, ProductRoom } from "./product-chat.js";
import { ProductRoomView, type ProductRoomDeps } from "./product-room-view.js";
import type { ProductWorkers } from "./product-workers.js";
import { ChatKeys } from "./chat/keys.js";
import { StatementStore } from "./chat/statement-store.js";
import {
  chatRecordKey,
  type Contact,
  type IncomingRequest,
  type StoredMessage,
} from "./chat/store.js";

/**
 * The host's own Chat screen, laid out like the Polkadot app's: a large
 * "Chats" title, message requests, the chat list, and a conversation with
 * bubbles and a composer. It is drawn by the host, not by a product, and is
 * not offered through TrUAPI.
 *
 * Chat runs for the signed-in session whether or not the screen is shown:
 * one engine per wallet, network and identity, started when the session
 * connects and stopped when it ends or changes. Accepting and sending publish
 * when the user does them, and received messages are acknowledged as they
 * arrive. The keys are derived in this page.
 */

/** What the screen needs from the signed-in session. All of it is public except the entropy. */
export interface ChatSession {
  walletId: string;
  entropy: Uint8Array;
  network: string;
  networkSuffix: string;
  /** What the core reports for the session, to check the host derives the same keys. */
  identityAccountId: HexString | undefined;
  chatPublicKey: HexString | undefined;
  /** The username the host found for the session on the People chain, to show when Asset Hub names none. */
  username: string | undefined;
  chain: ChainProvider;
  genesis: { assetHub: HexString; people: HexString };
}

type View =
  | { kind: "list" }
  | { kind: "new" }
  | { kind: "chat"; account: HexString }
  | { kind: "workers" }
  | { kind: "product-room"; productId: string; roomId: string };

/** The product side of the Chat screen: rooms product workers keep, and the workers. */
export interface ProductChats extends ProductRoomDeps {
  chat: ProductChat;
  workers: ProductWorkers;
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

const abbreviate = (account: string): string =>
  `${account.slice(0, 8)}…${account.slice(-6)}`;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const AVATAR_COLORS = [
  "#e6007a",
  "#56f39a",
  "#ffd37f",
  "#7f9dff",
  "#ff8f6b",
  "#b98cff",
];

function avatar(
  account: string,
  name: string | null,
  size: "large" | "small" = "large",
): HTMLElement {
  const node = el(
    "span",
    `chat-avatar chat-avatar-${size}`,
    (name ?? "?").slice(0, 1).toUpperCase(),
  );
  const hue = Number.parseInt(account.slice(2, 4), 16) % AVATAR_COLORS.length;
  node.style.setProperty("--avatar", AVATAR_COLORS[hue]);
  node.setAttribute("aria-hidden", "true");
  return node;
}

const ICON = /^(?:https:\/\/|data:image\/(?:png|jpeg|gif|webp);base64,)/;

/** A product room's icon when it is an `https` or inline raster image, else its initial. */
function productAvatar(
  room: ProductRoom,
  size: "large" | "small" = "large",
): HTMLElement {
  if (ICON.test(room.icon)) {
    const image = el("img", `chat-avatar chat-avatar-${size} product-avatar`);
    image.src = room.icon;
    image.alt = "";
    image.referrerPolicy = "no-referrer";
    return image;
  }
  const seed = [...room.productId].reduce(
    (sum, char) => sum + char.charCodeAt(0),
    0,
  );
  return avatar(
    `0x${(seed % 256).toString(16).padStart(2, "0")}`,
    room.name,
    size,
  );
}

function time(at: number): string {
  return new Date(at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function displayName(entry: {
  name: string | null;
  account?: string;
  peer?: string;
}): string {
  return entry.name ?? abbreviate(entry.account ?? entry.peer ?? "");
}

function preview(contact: Contact): string {
  if (contact.state === "requested")
    return "Waiting for them to accept your request";
  const last = contact.messages.at(-1);
  if (last === undefined) return "";
  if (last.kind === "accepted")
    return last.direction === "out"
      ? "You accepted the request"
      : "Accepted your request";
  return `${last.direction === "out" ? "You: " : ""}${last.text}`;
}

export class ChatScreen {
  private engine: ChatEngine | null = null;
  private sessionKey: string | null = null;
  private session: ChatSession | null = null;
  private problem: string | null = null;
  private view: View = { kind: "list" };
  private draft = "";
  private newTarget = "";
  private newWelcome = "";
  private formError: string | null = null;
  private busy = false;
  private visible = false;
  private stopListening: (() => void) | null = null;
  private productRoom: ProductRoomView | null = null;
  private workerName = "";
  private workerError: string | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly storage: Storage,
    private readonly diagnostics: ChatDiagnostics,
    private readonly onActivity: (pendingRequests: number) => void,
    private readonly products: ProductChats,
  ) {
    root.dataset.chatEngine = "off";
    products.chat.onChange(() => this.render());
    products.workers.onChange(() => this.render());
  }

  /** Follow the signed-in session. A different wallet, network or identity ends the old engine. */
  setSession(session: ChatSession | null): void {
    const key =
      session === null
        ? null
        : `${session.network}:${session.walletId}:${session.identityAccountId ?? ""}:${session.chatPublicKey ?? ""}`;
    this.session = session;
    if (key === this.sessionKey) {
      this.render();
      return;
    }
    this.sessionKey = key;
    this.stopEngine();
    // Events of one session are not shown in the next.
    this.diagnostics.clear();
    this.leaveProductRoom();
    this.view = { kind: "list" };
    this.draft = this.newTarget = this.newWelcome = this.workerName = "";
    this.workerError = null;
    if (session !== null) this.startEngine(session);
    this.render();
  }

  /** Show or hide the screen. Chat keeps running while it is hidden. */
  show(visible: boolean): void {
    this.visible = visible;
    this.root.hidden = !visible;
    this.render();
  }

  /** Stop the engine and forget its keys. */
  private stopEngine(): void {
    this.stopListening?.();
    this.stopListening = null;
    this.engine?.stop();
    this.engine = null;
    this.problem = null;
    this.formError = null;
    this.root.dataset.chatEngine = "off";
    this.onActivity(0);
  }

  /** Derive the session's keys, check them against the core, and listen. */
  private startEngine(session: ChatSession): void {
    this.problem = chatRefusal(session.entropy);
    if (this.problem !== null) return;
    let keys: ChatKeys;
    try {
      keys = ChatKeys.fromEntropy(session.entropy, session.networkSuffix);
    } catch (error) {
      this.problem = `Chat keys could not be derived: ${errorText(error)}`;
      return;
    }
    const mismatch = [
      session.identityAccountId !== undefined &&
      session.identityAccountId !== bytesToHex(keys.identity)
        ? "identity account"
        : null,
      session.chatPublicKey !== undefined &&
      session.chatPublicKey !== bytesToHex(keys.chatPublic)
        ? "chat key"
        : null,
    ].filter((part) => part !== null);
    if (
      session.identityAccountId === undefined ||
      session.chatPublicKey === undefined
    )
      this.problem =
        "The core did not report this session's identity account and chat key, so chat cannot check its keys.";
    else if (mismatch.length > 0)
      this.problem = `The host derived a different ${mismatch.join(" and ")} than the core. Chat stays off.`;
    if (this.problem !== null) return;
    const engine = new ChatEngine({
      keys,
      openStore: async () =>
        new StatementStore(
          await session.chain.connect(hexToBytes(session.genesis.people)),
        ),
      directory: chainDirectory(session.chain, session.genesis),
      storage: this.storage,
      recordKey: chatRecordKey(session.network, session.walletId),
      writes: () => true,
      diagnostics: this.diagnostics.record,
    });
    this.engine = engine;
    this.stopListening = engine.onChange(() => {
      this.root.dataset.chatEngine = engine.snapshot().status.state;
      this.onActivity(engine.snapshot().requests.length);
      this.render();
    });
    void engine.start();
  }

  private act(
    work: () => Promise<void>,
    onError: (message: string) => void = (message) =>
      (this.formError = message),
  ): void {
    this.busy = true;
    this.formError = null;
    this.render();
    work()
      .catch((error: unknown) => onError(errorText(error)))
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
    const log = this.root.querySelector(".chat-thread");
    if (log !== null) log.scrollTop = log.scrollHeight;
  }

  private draw(): HTMLElement[] {
    if (this.session === null)
      return [
        this.bar("Chats"),
        this.empty(
          "Sign in to chat",
          "Choose a wallet in the menu. Chat uses that wallet's identity.",
        ),
      ];
    if (this.view.kind === "product-room") {
      const room = this.products.chat.room(
        this.view.productId,
        this.view.roomId,
      );
      if (room !== undefined) return this.drawProductRoom(room);
      this.leaveProductRoom();
      this.view = { kind: "list" };
    }
    if (this.view.kind === "workers") return this.drawWorkers();
    const engine = this.engine;
    if (engine === null) {
      const body = el("div", "chat-body");
      body.append(
        this.empty("Chat is unavailable", this.problem ?? ""),
        this.drawProductSection(),
      );
      return [this.bar("Chats"), body];
    }
    const snapshot = engine.snapshot();
    if (this.view.kind === "chat") {
      const account = this.view.account;
      const contact = snapshot.contacts.find(
        (candidate) => candidate.account === account,
      );
      if (contact !== undefined)
        return this.drawConversation(snapshot, contact);
      this.view = { kind: "list" };
    }
    if (this.view.kind === "new") return this.drawNewChat(snapshot);
    return this.drawList(snapshot);
  }

  private leaveProductRoom(): void {
    this.productRoom?.dispose();
    this.productRoom = null;
  }

  private back(): HTMLButtonElement {
    const back = button("chat-icon-button", "‹", () => {
      this.leaveProductRoom();
      this.view = { kind: "list" };
      this.render();
    });
    back.setAttribute("aria-label", "Back to chats");
    return back;
  }

  /** The rooms product workers keep, and the way to manage the workers. */
  private drawProductSection(): HTMLElement {
    const section = el("section", "chat-section product-section");
    const head = el("div", "product-section-head");
    const manage = button("chat-link", "Workers", () => {
      this.view = { kind: "workers" };
      this.render();
    });
    manage.dataset.action = "manage-workers";
    head.append(el("h3", "chat-section-title", "Product chats"), manage);
    section.append(head);
    const rooms = [...this.products.chat.rooms()].sort((left, right) => {
      const last = (room: ProductRoom) =>
        this.products.chat.messages(room.productId, room.roomId).at(-1)?.at ??
        room.createdAt;
      return last(right) - last(left);
    });
    if (rooms.length === 0) {
      section.append(
        el(
          "p",
          "chat-identity-line",
          "No product rooms yet. Add a product's worker to chat with it.",
        ),
      );
      return section;
    }
    const list = el("ul", "chat-list");
    for (const room of rooms) list.append(this.drawProductRoomRow(room));
    section.append(list);
    return section;
  }

  private drawProductRoomRow(room: ProductRoom): HTMLElement {
    const item = el("li", "");
    const row = button("chat-row chat-row-button", "", () => {
      this.leaveProductRoom();
      this.view = {
        kind: "product-room",
        productId: room.productId,
        roomId: room.roomId,
      };
      this.render();
    });
    row.dataset.productRoom = `${room.productId}/${room.roomId}`;
    const last = this.products.chat
      .messages(room.productId, room.roomId)
      .at(-1);
    const text = el("span", "chat-row-text");
    text.append(
      el("span", "chat-row-title", room.name),
      el(
        "span",
        "chat-row-preview",
        last === undefined
          ? room.productId
          : last.content.tag === "Text"
            ? last.content.text
            : (last.content.alt ?? room.productId),
      ),
    );
    const state = this.products.workers.state(room.productId)?.state;
    row.append(
      productAvatar(room),
      text,
      el(
        "span",
        "chat-row-time",
        state === "running" ? (last ? time(last.at) : "") : "offline",
      ),
    );
    item.append(row);
    return item;
  }

  private drawProductRoom(room: ProductRoom): HTMLElement[] {
    if (
      this.productRoom === null ||
      this.productRoom.productId !== room.productId ||
      this.productRoom.roomId !== room.roomId
    ) {
      this.leaveProductRoom();
      this.productRoom = new ProductRoomView(
        room.productId,
        room.roomId,
        this.products,
      );
    }
    this.productRoom.refresh();
    const bar = el("header", "chat-bar chat-bar-small");
    const heading = el("div", "chat-row-text");
    heading.append(
      el("span", "chat-row-title", room.name),
      el("span", "chat-row-preview", room.productId),
    );
    bar.append(this.back(), productAvatar(room, "small"), heading);
    return [bar, this.productRoom.thread, this.productRoom.footer];
  }

  /** The workers added for this wallet, their state, and the form to add one. */
  private drawWorkers(): HTMLElement[] {
    const bar = el("header", "chat-bar chat-bar-small");
    bar.append(this.back(), el("h2", "chat-title chat-title-small", "Workers"));
    const body = el("div", "chat-body");
    const form = el("form", "chat-form product-worker-form");
    const input = el("input", "chat-input");
    input.dataset.focus = "worker-name";
    input.placeholder = "Product name, such as myapp.paseo";
    input.value = this.workerName;
    input.autocomplete = "off";
    input.addEventListener("input", () => (this.workerName = input.value));
    const add = el("button", "chat-pill chat-pill-primary", "Add worker");
    add.type = "submit";
    add.disabled = this.busy;
    form.append(
      el("label", "chat-label", "Run a product's worker"),
      input,
      add,
    );
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const name = this.workerName;
      this.busy = true;
      this.workerError = null;
      this.render();
      this.products.workers
        .add(name)
        .then(() => (this.workerName = ""))
        .catch((error: unknown) => (this.workerError = errorText(error)))
        .finally(() => {
          this.busy = false;
          this.render();
        });
    });
    if (this.workerError !== null)
      form.append(el("p", "chat-error", this.workerError));
    body.append(form);
    const list = el("ul", "product-worker-list");
    for (const name of this.products.workers.names()) {
      const state = this.products.workers.state(name);
      const item = el("li", "product-worker");
      item.dataset.worker = name;
      item.dataset.state = state?.state ?? "stopped";
      const remove = button("chat-link", "Remove", () =>
        this.products.workers.remove(name),
      );
      item.append(
        el("span", "chat-row-title", name),
        el(
          "span",
          "chat-row-preview",
          state === null
            ? "Stopped"
            : state.state === "running"
              ? `Running, version ${state.manifest.appVersion.join(".")}`
              : state.state === "starting"
                ? "Starting…"
                : `Failed: ${state.reason}`,
        ),
        remove,
      );
      list.append(item);
    }
    body.append(
      list,
      el(
        "p",
        "chat-identity-line",
        "A worker runs while this wallet is signed in and this tab is open. It is the product's own code, on this host's origin.",
      ),
    );
    return [bar, body];
  }

  private bar(title: string, action?: HTMLElement): HTMLElement {
    const bar = el("header", "chat-bar");
    bar.append(el("h2", "chat-title", title));
    if (action) bar.append(action);
    return bar;
  }

  private empty(title: string, body: string): HTMLElement {
    const node = el("div", "chat-empty");
    node.append(
      el("p", "chat-empty-title", title),
      el("p", "chat-empty-body", body),
    );
    return node;
  }

  /** Why nothing can be published now, or null when it can. */
  private sendBlock(snapshot: ChatSnapshot): string | null {
    return snapshot.canSend === false ? "No statement allowance." : null;
  }

  private identity(snapshot: ChatSnapshot): HTMLElement {
    const strip = el("div", "chat-identity");
    strip.dataset.status = snapshot.status.state;
    const status =
      snapshot.status.state === "listening"
        ? "Listening"
        : snapshot.status.state === "starting"
          ? "Connecting…"
          : snapshot.status.state === "failed"
            ? `Not connected: ${snapshot.status.reason}`
            : "Stopped";
    const line = el("p", "chat-identity-line");
    line.append(el("span", "chat-dot"), el("span", "", status));
    const who = el(
      "p",
      "chat-identity-who",
      `${snapshot.name ?? this.session?.username ?? "No username"} · ${abbreviate(snapshot.account)}`,
    );
    who.title = `Identity account ${snapshot.account}\nChat key ${snapshot.chatKey}`;
    strip.append(line, who);
    const notes: string[] = [];
    if (snapshot.chainKey === "unknown")
      strip.append(el("p", "chat-identity-line", "Checking chain…"));
    if (snapshot.chainKey === "missing")
      notes.push(
        "No chat key on chain: no one can send this identity a request.",
      );
    if (snapshot.chainKey === "differs")
      notes.push("The chat key on chain is not this wallet's.");
    if (snapshot.chainKey === "conflict")
      notes.push("Different chat keys on Asset Hub and the People chain.");
    if (snapshot.chainKey === "unreadable")
      notes.push("The chat key on chain could not be read.");
    if (snapshot.canSend === false)
      notes.push(
        "No statement allowance: this identity cannot answer or send.",
      );
    for (const note of notes) strip.append(el("p", "chat-note", note));
    return strip;
  }

  private drawList(snapshot: ChatSnapshot): HTMLElement[] {
    const add = button("chat-icon-button", "+", () => {
      this.view = { kind: "new" };
      this.formError = null;
      this.render();
    });
    add.setAttribute("aria-label", "New chat");
    add.title = "New chat";
    const body = el("div", "chat-body");
    body.append(this.identity(snapshot));
    if (snapshot.requests.length > 0) {
      const section = el("section", "chat-section");
      section.append(el("h3", "chat-section-title", "Message requests"));
      for (const request of snapshot.requests)
        section.append(this.drawRequest(request));
      body.append(section);
    }
    if (snapshot.contacts.length === 0 && snapshot.requests.length === 0)
      body.append(
        this.empty(
          "Add your first contact",
          "Tap + to send a chat request by username.",
        ),
      );
    else if (snapshot.contacts.length > 0) {
      const list = el("ul", "chat-list");
      const ordered = [...snapshot.contacts].sort(
        (left, right) =>
          (right.messages.at(-1)?.at ?? 0) - (left.messages.at(-1)?.at ?? 0),
      );
      for (const contact of ordered) list.append(this.drawContact(contact));
      body.append(list);
    }
    body.append(this.drawProductSection());
    if (this.formError !== null)
      body.append(el("p", "chat-error", this.formError));
    if (snapshot.skipped.length > 0) {
      const details = el("details", "chat-skipped");
      details.append(
        el("summary", "", `${snapshot.skipped.length} statements set aside`),
      );
      for (const reason of snapshot.skipped)
        details.append(el("p", "", reason));
      body.append(details);
    }
    return [this.bar("Chats", add), body];
  }

  private drawRequest(request: IncomingRequest): HTMLElement {
    const card = el("article", "chat-request");
    card.dataset.request = request.id;
    const head = el("div", "chat-row");
    const text = el("div", "chat-row-text");
    text.append(
      el("span", "chat-row-title", displayName(request)),
      el("span", "chat-row-preview", request.welcome ?? "Wants to chat"),
    );
    head.append(
      avatar(request.peer, request.name),
      text,
      el("span", "chat-row-time", time(request.at)),
    );
    card.append(head);
    if (request.problem !== null)
      card.append(el("p", "chat-note", request.problem));
    const actions = el("div", "chat-request-actions");
    const accept = button("chat-pill chat-pill-primary", "Accept", () =>
      this.act(async () => {
        await this.engine?.accept(request.id);
        this.view = { kind: "chat", account: request.peer };
      }),
    );
    accept.disabled = this.busy || request.problem !== null;
    const decline = button("chat-pill", "Decline", () =>
      this.engine?.decline(request.id),
    );
    decline.disabled = this.busy;
    actions.append(decline, accept);
    card.append(actions);
    return card;
  }

  private drawContact(contact: Contact): HTMLElement {
    const item = el("li", "");
    const row = button("chat-row chat-row-button", "", () => {
      this.view = { kind: "chat", account: contact.account };
      this.draft = "";
      this.formError = null;
      this.render();
    });
    row.dataset.contact = contact.account;
    const text = el("span", "chat-row-text");
    text.append(
      el("span", "chat-row-title", displayName(contact)),
      el("span", "chat-row-preview", preview(contact)),
    );
    const last = contact.messages.at(-1);
    row.append(
      avatar(contact.account, contact.name),
      text,
      el("span", "chat-row-time", last ? time(last.at) : ""),
    );
    item.append(row);
    return item;
  }

  private drawNewChat(snapshot: ChatSnapshot): HTMLElement[] {
    const back = button("chat-icon-button", "‹", () => {
      this.view = { kind: "list" };
      this.render();
    });
    back.setAttribute("aria-label", "Back to chats");
    const form = el("form", "chat-form");
    const target = el("input", "chat-input");
    target.dataset.focus = "target";
    target.placeholder = "Username, such as name.01, or a 0x account id";
    target.value = this.newTarget;
    target.autocomplete = "off";
    target.addEventListener("input", () => (this.newTarget = target.value));
    const welcome = el("textarea", "chat-input chat-textarea");
    welcome.dataset.focus = "welcome";
    welcome.placeholder = "First message";
    welcome.value = this.newWelcome;
    welcome.addEventListener("input", () => (this.newWelcome = welcome.value));
    const submit = el("button", "chat-pill chat-pill-primary", "Send request");
    submit.type = "submit";
    const blocked = this.sendBlock(snapshot);
    submit.disabled =
      this.busy || snapshot.status.state !== "listening" || blocked !== null;
    form.append(
      el("label", "chat-label", "To"),
      target,
      el("label", "chat-label", "Message"),
      welcome,
      submit,
    );
    if (blocked !== null) form.append(el("p", "chat-note", blocked));
    if (this.formError !== null)
      form.append(el("p", "chat-error", this.formError));
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      this.act(async () => {
        await this.engine?.startChat(this.newTarget, this.newWelcome.trim());
        const sent = this.engine?.snapshot().contacts.at(-1);
        this.newTarget = this.newWelcome = "";
        this.view = sent
          ? { kind: "chat", account: sent.account }
          : { kind: "list" };
      });
    });
    const bar = el("header", "chat-bar chat-bar-small");
    bar.append(back, el("h2", "chat-title chat-title-small", "New chat"));
    return [bar, form];
  }

  private drawConversation(
    snapshot: ChatSnapshot,
    contact: Contact,
  ): HTMLElement[] {
    const back = button("chat-icon-button", "‹", () => {
      this.view = { kind: "list" };
      this.formError = null;
      this.render();
    });
    back.setAttribute("aria-label", "Back to chats");
    const bar = el("header", "chat-bar chat-bar-small");
    const heading = el("div", "chat-row-text");
    const sub = el("span", "chat-row-preview", abbreviate(contact.account));
    sub.title = contact.account;
    heading.append(el("span", "chat-row-title", displayName(contact)), sub);
    bar.append(back, avatar(contact.account, contact.name, "small"), heading);

    const thread = el("div", "chat-thread");
    thread.setAttribute("role", "log");
    for (const message of contact.messages)
      thread.append(this.drawMessage(contact, message));

    const footer = el("div", "chat-footer");
    if (contact.state === "requested")
      footer.append(
        el(
          "p",
          "chat-waiting",
          `Wait for ${displayName(contact)} to accept your message request.`,
        ),
      );
    else {
      const form = el("form", "chat-composer");
      const input = el("textarea", "chat-input chat-composer-input");
      input.dataset.focus = "composer";
      input.rows = 1;
      const blocked = this.sendBlock(snapshot);
      input.placeholder = blocked ?? "Message";
      input.value = this.draft;
      input.disabled = blocked !== null;
      input.addEventListener("input", () => (this.draft = input.value));
      input.addEventListener("keydown", (event) => {
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          form.requestSubmit();
        }
      });
      const send = el("button", "chat-send", "Send");
      send.type = "submit";
      send.disabled = blocked !== null;
      form.append(input, send);
      form.addEventListener("submit", (event) => {
        event.preventDefault();
        const text = this.draft.trim();
        if (text === "") return;
        this.draft = "";
        this.act(
          () => this.engine?.send(contact.account, text) ?? Promise.resolve(),
        );
      });
      footer.append(form);
    }
    if (this.formError !== null)
      footer.append(el("p", "chat-error", this.formError));
    return [bar, thread, footer];
  }

  private drawMessage(contact: Contact, message: StoredMessage): HTMLElement {
    if (message.kind === "accepted" || message.kind === "unsupported") {
      const text =
        message.kind === "unsupported"
          ? message.text
          : message.direction === "out"
            ? `You accepted the chat request${message.status === "failed" ? ", but it was not sent" : ""}`
            : `${displayName(contact)} accepted your chat request`;
      const node = el("p", "chat-system", text);
      if (message.problem) node.title = message.problem;
      if (message.status === "failed") node.append(this.retry(contact));
      return node;
    }
    const row = el("div", `chat-message chat-message-${message.direction}`);
    row.dataset.status = message.status;
    const bubble = el("div", "chat-bubble");
    if (message.kind === "request")
      bubble.append(
        el(
          "span",
          "chat-bubble-label",
          message.direction === "in" ? "Chat request" : "Your chat request",
        ),
      );
    bubble.append(
      el(
        "span",
        "chat-bubble-text",
        message.text === "" ? "(no message)" : message.text,
      ),
    );
    const meta = el("span", "chat-bubble-meta", time(message.at));
    if (message.direction === "out")
      meta.append(el("span", "chat-tick", statusMark(message)));
    bubble.append(meta);
    row.append(bubble);
    if (message.status === "failed") {
      const failed = el(
        "p",
        "chat-failed",
        `Not sent: ${message.problem ?? "unknown reason"} `,
      );
      failed.append(this.retry(contact));
      row.append(failed);
    }
    return row;
  }

  private retry(contact: Contact): HTMLButtonElement {
    const retry = button("chat-link", "Retry", () =>
      this.act(() => this.engine?.retry(contact.account) ?? Promise.resolve()),
    );
    retry.disabled = this.busy;
    return retry;
  }
}

function statusMark(message: StoredMessage): string {
  switch (message.status) {
    case "pending":
      return " · sending";
    case "sent":
      return " · sent";
    case "acknowledged":
      return " · acknowledged";
    case "failed":
      return " · failed";
    default:
      return "";
  }
}
