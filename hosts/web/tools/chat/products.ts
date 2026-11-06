import "./products.css";
import { ask } from "../../src/app/prompt.js";
import type { PeopleAccess } from "../statements/actions.js";
import { finalizedAllowance, kettleChat } from "./kettle.js";
import {
  BAD_RELAY_URL,
  relayUrlOf,
  type IrohRelays,
} from "../../src/chat/kettle-chat/route.js";
import type { WorkerState } from "../../src/products/product-workers.js";
import {
  CONTACT_CHAT,
  type ChatProductSpec,
  type ContactChatOwner,
} from "../../src/invitations/wallet-chat.js";

/**
 * The developer tools' Chat tab: which chat products this tab runs
 * (Contact Chat, Kettle Chat, or both), which one New chat starts in, and
 * what each runs now. How host requests travel is the request service's, not
 * Chat's: More holds it.
 *
 * Each chat product is a product worker of its own, with its own product id,
 * request service, request lists, storage and accounts. A product that is off
 * does not start: no worker, no request service, no delivery and no reposts,
 * so none of its chats show in Chat. Nothing of it is deleted: turning it on
 * again starts it from what it kept. Every change reloads the tab, so a
 * product never stops halfway.
 *
 * The dev server always has it. A build has it only when made with
 * `WEB_SIGNING_KETTLE_CHAT=1` (`contact-chat-plugin.ts`), for a private test
 * deployment: the host's page loads this module only then.
 */

/** The chat products the tab can run, in the order the tab lists them. */
export const CHAT_PRODUCTS = ["contact-chat", "kettle-chat"] as const;
export type ChatProductKey = (typeof CHAT_PRODUCTS)[number];

type ChoiceStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/** sessionStorage key: `off` while Contact Chat does not run in this tab. Absent: it runs. */
const CONTACT_CHAT_KEY = "truapi-web-signing-host:dev:contact-chat";
/** sessionStorage key: `on` while Kettle Chat runs in this tab. Absent: it does not. */
const KETTLE_CHAT_KEY = "truapi-web-signing-host:dev:kettle-chat";
/**
 * sessionStorage key: the product New chat starts in while both run.
 * Absent: Contact Chat.
 */
const NEW_CHATS_KEY = "truapi-web-signing-host:dev:new-chats";
/**
 * sessionStorage key: Kettle Chat's log recovery over iroh, `public` for
 * n0's public test relays or one custom relay's URL. Absent: off, with no
 * iroh endpoint and no relay connection.
 */
const KETTLE_IROH_KEY = "truapi-web-signing-host:dev:kettle-iroh-relay";
/** The delivery key, whose `off` turned every chat product off before this tab had switches. */
const WALLET_DELIVERY_KEY = "truapi-web-signing-host:dev:wallet-delivery";

/** Whether `key` runs in this tab. */
export function chatProductOn(
  storage: ChoiceStorage,
  key: ChatProductKey,
): boolean {
  return key === "contact-chat"
    ? storage.getItem(CONTACT_CHAT_KEY) !== "off"
    : storage.getItem(KETTLE_CHAT_KEY) === "on";
}

/** The chat products that run in this tab, the one New chat starts in first. */
export function chatProductsOn(storage: ChoiceStorage): ChatProductKey[] {
  const on = CHAT_PRODUCTS.filter((key) => chatProductOn(storage, key));
  const first =
    storage.getItem(NEW_CHATS_KEY) === "kettle-chat"
      ? "kettle-chat"
      : "contact-chat";
  return on.includes(first)
    ? [first, ...on.filter((key) => key !== first)]
    : on;
}

export function setChatProductOn(
  storage: ChoiceStorage,
  key: ChatProductKey,
  on: boolean,
): void {
  if (key === "contact-chat") {
    if (on) storage.removeItem(CONTACT_CHAT_KEY);
    else storage.setItem(CONTACT_CHAT_KEY, "off");
  } else if (on) storage.setItem(KETTLE_CHAT_KEY, "on");
  else storage.removeItem(KETTLE_CHAT_KEY);
}

/** New chat starts in `key` while it runs beside another. */
export function setNewChats(storage: ChoiceStorage, key: ChatProductKey): void {
  if (key === "contact-chat") storage.removeItem(NEW_CHATS_KEY);
  else storage.setItem(NEW_CHATS_KEY, key);
}

/** Kettle Chat's iroh relays in this tab, or null: off. A stored value that does not read is off. */
export function kettleIroh(storage: ChoiceStorage): IrohRelays | null {
  const value = storage.getItem(KETTLE_IROH_KEY);
  if (value === null) return null;
  if (value === "public") return { relays: "public" };
  const relayUrl = relayUrlOf(value);
  return relayUrl === null ? null : { relays: "custom", relayUrl };
}

/**
 * Run Kettle Chat's log transfer on `iroh`, or none with null. Throws on a
 * custom relay {@link relayUrlOf} refuses, without repeating it.
 */
export function setKettleIroh(
  storage: ChoiceStorage,
  iroh: IrohRelays | null,
): void {
  if (iroh === null) return storage.removeItem(KETTLE_IROH_KEY);
  if (iroh.relays === "public")
    return storage.setItem(KETTLE_IROH_KEY, "public");
  const url = relayUrlOf(iroh.relayUrl);
  if (url === null) throw new Error(BAD_RELAY_URL);
  storage.setItem(KETTLE_IROH_KEY, url);
}

/**
 * A tab whose delivery was `off` ran no chat product. It keeps running none:
 * both switches turn off and the delivery goes back to its default. Answers
 * whether it moved anything.
 */
export function moveDeliveryOff(storage: ChoiceStorage): boolean {
  if (storage.getItem(WALLET_DELIVERY_KEY) !== "off") return false;
  storage.removeItem(WALLET_DELIVERY_KEY);
  setChatProductOn(storage, "contact-chat", false);
  setChatProductOn(storage, "kettle-chat", false);
  return true;
}

/** What the tab reads of the chat products that run, once the page has them. */
export interface RunningChats {
  owner: ContactChatOwner;
  workers: {
    state(name: string): WorkerState | null;
    onChange(listener: () => void): () => void;
  };
  /** Show `name`'s worker in the inspector. */
  inspect(name: string): void;
}

export interface ChatTabDeps {
  panel: HTMLElement;
  tab: HTMLElement;
  networkSuffix: string;
  specs: Record<ChatProductKey, ChatProductSpec>;
}

export interface ChatTab {
  /** Show the state of the chat products the page runs. */
  follow(running: RunningChats): void;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== "") node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const plural = (count: number, one: string) =>
  `${count} ${one}${count === 1 ? "" : "s"}`;

function stateLine(state: WorkerState | null): string {
  if (state === null) return "Starting…";
  if (state.state === "running")
    return `Running · v${state.manifest.appVersion.join(".")}`;
  if (state.state === "starting") return "Starting…";
  return `Failed: ${state.reason}`;
}

/** What turning `name` off leaves waiting, asked first when it has sent requests waiting. */
function confirmOff(name: string, waiting: number): Promise<boolean> {
  return ask<boolean>({
    title: `Turn ${name} off?`,
    fields: [
      {
        label: "Waiting",
        value: `${plural(waiting, "sent request")}. They stay pending, but this tab does not post them again or read their replies while ${name} is off.`,
      },
      {
        label: "Keeps",
        value: `${name}'s chats, history, requests and accounts. Turning it on again resumes them.`,
      },
    ],
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Turn off", value: true, tone: "destructive" },
    ],
    dismissed: false,
  });
}

/**
 * Kettle Chat's log recovery over iroh: off unless turned on here, then on
 * n0's public test relays or one custom relay. The product runs the
 * endpoints; this tab only passes it the choice.
 */
function irohSection(): HTMLElement {
  const section = el("section", "dev-group");
  section.setAttribute("aria-labelledby", "kettle-iroh-title");
  const title = el("h2", "", "Kettle Chat: recover missed messages");
  title.id = "kettle-iroh-title";
  const current = kettleIroh(sessionStorage);
  const intro = el(
    "p",
    "note",
    "Off by default. On, a chat that missed messages fetches them from the sender through an iroh relay, while the sender's tab is open.",
  );
  const box = el("input");
  box.type = "checkbox";
  box.id = "kettle-iroh";
  box.checked = current !== null;
  const boxLabel = el("label", "chat-product-switch");
  boxLabel.append(box, " Recover missed messages over iroh");
  const relaysLabel = el("label", "", "Relays");
  const relays = el("select");
  relays.id = "kettle-iroh-relays";
  relaysLabel.htmlFor = relays.id;
  relays.append(
    new Option("Use public test relays", "public"),
    new Option("Custom relay (advanced)", "custom"),
  );
  relays.value = current?.relays ?? "public";
  const relayLabel = el("label", "", "Relay URL");
  const relay = el("input");
  relay.type = "url";
  relay.id = "kettle-iroh-relay";
  relay.placeholder = "https://relay.example";
  relay.value = current?.relays === "custom" ? current.relayUrl : "";
  relayLabel.htmlFor = relay.id;
  const sync = () => {
    relays.disabled = !box.checked;
    const custom = relays.value === "custom";
    relayLabel.hidden = !custom;
    relay.hidden = !custom;
    relay.disabled = !box.checked;
  };
  for (const input of [box, relays]) input.addEventListener("change", sync);
  sync();
  const apply = el("button", "", "Apply and reload");
  apply.type = "button";
  apply.id = "kettle-iroh-apply";
  const status = el(
    "p",
    "note",
    current === null
      ? "Now: off."
      : `Now: on · ${current.relays === "public" ? "public test relays" : `custom relay ${current.relayUrl}`}. Inspector → Chat shows each chat's state.`,
  );
  status.id = "kettle-iroh-status";
  apply.addEventListener("click", () => {
    try {
      setKettleIroh(
        sessionStorage,
        !box.checked
          ? null
          : relays.value === "custom"
            ? { relays: "custom", relayUrl: relay.value }
            : { relays: "public" },
      );
    } catch (error) {
      status.textContent =
        error instanceof Error ? error.message : String(error);
      return;
    }
    window.location.reload();
  });
  section.append(
    title,
    intro,
    boxLabel,
    relaysLabel,
    relays,
    relayLabel,
    relay,
    apply,
    status,
    fold("What this shares and costs", [
      "Every connection goes through a relay: browsers do not connect to each other directly.",
      "Messages, keys and invitations still travel on the Statement Store. Chats work without iroh.",
      "Fetched messages stay end-to-end encrypted: the relay cannot read them.",
      "The public test relays are n0's, for testing: shared, rate-limited and without guarantee.",
      "The relay sees this browser's IP address, connection ids, times and sizes.",
      "Each chat publishes its relay's address and a random id with its messages. A custom relay that few people use can link your chats.",
      "Only the chosen relays are used. A contact on another relay gets no connection.",
      "A custom relay URL is a scheme, a host and a port only, such as https://relay.example. It is published, so it must hold no secret.",
      "If no relay answers within 10 s, that chat waits that long to start, then runs without iroh until the next reload.",
      "With iroh in the build, Kettle Chat downloads about 0.8 MB more (compressed) and keeps more in memory, even when this is off.",
      "Turning it on or off reloads the tab. Chats and history stay.",
    ]),
  );
  return section;
}

/** A closed fold of short notes, for what a section's controls need not show at once. */
function fold(summary: string, notes: readonly string[]): HTMLElement {
  const details = el("details", "fold");
  details.append(
    el("summary", "", summary),
    ...notes.map((note) => el("p", "note", note)),
  );
  return details;
}

export function mountChatTab(deps: ChatTabDeps): ChatTab {
  const { panel, networkSuffix, specs } = deps;
  const product = (key: ChatProductKey) => specs[key].product(networkSuffix);
  let running: RunningChats | null = null;

  const productsSection = el("section", "dev-group");
  productsSection.setAttribute("aria-labelledby", "chat-products-title");
  const title = el("h2", "", "Chat products");
  title.id = "chat-products-title";
  const intro = el(
    "p",
    "note",
    "Each is a product worker of its own: its own id, requests, storage and accounts. Only the ones on run in this tab, and Chat shows only their chats. Changing this reloads the tab.",
  );
  const list = el("ul", "worker-list");
  list.id = "chat-products";

  const rows = CHAT_PRODUCTS.map((key) => {
    const { id, name } = product(key);
    const item = el("li");
    item.dataset.chatProduct = key;
    const label = el("span", "worker-name", name);
    const facts = el(
      "span",
      "worker-state",
      `${id} · ${specs[key].messagesEncrypted ? "end-to-end encrypted" : "messages readable by anyone"}`,
    );
    const line = el("span", "worker-state");
    line.dataset.chatProductState = key;
    const inspect = el("button", "quiet", "Inspect");
    inspect.type = "button";
    inspect.setAttribute("aria-label", `Inspect ${name}'s worker`);
    inspect.addEventListener("click", () => running?.inspect(id));
    const toggle = el("label", "chat-product-switch");
    const box = el("input");
    box.type = "checkbox";
    box.id = `chat-product-${key}`;
    box.checked = chatProductOn(sessionStorage, key);
    box.setAttribute("aria-label", `Run ${name} in this tab`);
    toggle.append(box, " On");
    box.addEventListener("change", () => {
      const on = box.checked;
      const waiting = on ? 0 : waitingFor(id);
      void (
        waiting === 0 ? Promise.resolve(true) : confirmOff(name, waiting)
      ).then((confirmed) => {
        if (!confirmed) {
          box.checked = true;
          return;
        }
        setChatProductOn(sessionStorage, key, on);
        window.location.reload();
      });
    });
    item.append(label, facts, line, inspect, toggle);
    return { key, id, name, item, line, inspect, box };
  });
  list.append(...rows.map((row) => row.item));

  const newChats = el("select");
  newChats.id = "new-chat-product";
  const newChatsLabel = el("label", "", "New chats start in");
  newChatsLabel.htmlFor = newChats.id;
  const on = chatProductsOn(sessionStorage);
  if (on.length === 0) newChats.append(new Option("No chat product is on", ""));
  else newChats.append(...on.map((key) => new Option(product(key).name, key)));
  newChats.disabled = on.length < 2;
  newChats.title =
    "The product New chat sends a request through while both run. Chats already started stay in their own product. Switching reloads the tab.";
  newChats.addEventListener("change", () => {
    setNewChats(sessionStorage, newChats.value as ChatProductKey);
    window.location.reload();
  });
  const off = fold("Off keeps its chats and history", [
    "Nothing of it runs while it is off. Turning it on again resumes its chats.",
    "Its pending requests are not posted again while it is off.",
    "A request sent to it is read only if it is still posted when it is on again.",
  ]);
  productsSection.append(title, intro, list, newChatsLabel, newChats, off);

  panel.append(productsSection, irohSection());
  deps.tab.hidden = false;

  /** The sent requests of `productId` still waiting, as its running request service holds them. */
  function waitingFor(productId: string): number {
    return (running?.owner.notificationActions.sent?.() ?? []).filter(
      (request) => request.product.id === productId,
    ).length;
  }

  const render = () => {
    const runs = new Set(running?.owner.workerNames() ?? []);
    for (const row of rows) {
      const isOn = row.box.checked;
      const runsNow = runs.has(row.id);
      const state = runsNow ? (running?.workers.state(row.id) ?? null) : null;
      row.item.dataset.state = !isOn
        ? "off"
        : runsNow
          ? (state?.state ?? "starting")
          : "stopped";
      row.line.textContent = !isOn
        ? "Off · nothing runs"
        : !runsNow
          ? "On · not running yet: it runs for a signed-in wallet or demo user"
          : [
              stateLine(state),
              ...(waitingFor(row.id) > 0
                ? [`${plural(waitingFor(row.id), "sent request")} waiting`]
                : []),
            ].join(" · ");
      row.inspect.hidden = !runsNow;
    }
  };
  render();

  return {
    follow(next) {
      running = next;
      next.owner.notificationActions.onSentChange?.(render);
      next.workers.onChange(render);
      render();
    },
  };
}

/** The chat products this tab runs, and the Chat tab that chose them. */
export interface ChatModality {
  /** The chat products that run, the one New chat starts in first. */
  keys: readonly ChatProductKey[];
  /** Kettle Chat's iroh relays, or null: off. */
  kettleIroh: IrohRelays | null;
  /**
   * The signed-in wallet's chat products. Kettle Chat waits for each
   * conversation account's claim to finalize at the People chain read
   * through `people`.
   */
  walletProducts(people: () => PeopleAccess | null): readonly ChatProductSpec[];
  /** Show in the Chat tab the state of the chat products the page runs. */
  follow(running: RunningChats): void;
}

/** Mount the Chat tab and read which chat products run in this tab. */
export function startChatModality(deps: {
  panel: HTMLElement;
  tab: HTMLElement;
  networkSuffix: string;
  log(line: string): void;
}): ChatModality {
  if (moveDeliveryOff(sessionStorage))
    deps.log(
      "This tab's chat delivery was Off: every chat product is now off in the Chat tab instead.",
    );
  const keys = chatProductsOn(sessionStorage);
  const iroh = kettleIroh(sessionStorage);
  const tab = mountChatTab({
    panel: deps.panel,
    tab: deps.tab,
    networkSuffix: deps.networkSuffix,
    specs: { "contact-chat": CONTACT_CHAT, "kettle-chat": kettleChat() },
  });
  return {
    keys,
    kettleIroh: iroh,
    walletProducts: (people) =>
      keys.map((key) =>
        key === "contact-chat"
          ? CONTACT_CHAT
          : kettleChat(finalizedAllowance(people, deps.log), iroh),
      ),
    follow: (running) => tab.follow(running),
  };
}
