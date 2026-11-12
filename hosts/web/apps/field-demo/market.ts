import { getClientSync } from "@parity/truapi/sandbox";
import { ProductStorage } from "../shared/product-storage.js";
import { blake2b256 } from "../../bridges/primitives/crypto.js";
import { toHex, utf8 } from "../../bridges/primitives/bytes.js";
import {
  mountProtectedField,
  type ProtectedField,
} from "../../bridges/fields/protected-field.js";
import type {
  FieldDescriptor,
  ReceiveKey,
} from "../../bridges/fields/protocol.js";
import { connect, element, FieldError } from "./client.js";
import { openBoard, type Heard } from "./board.js";
import { orderContext, type Listing } from "./messages.js";
import { createToast, fillIcons, icon, node, paint } from "./ui.js";

/**
 * Small Market. It owns its listings, orders and their delivery on its
 * Statement Store board (`board.ts`), and keeps its own records in its
 * TrUAPI storage. It asks the host for a buyer's details through the draft
 * field bridge and gets only descriptors, handles and a sealed envelope;
 * the host draws the values where the market places them.
 */

/** A sale this market received for one of its own listings. */
interface Sale {
  /** The order id and the poster's account: another poster's copy of an id is another sale. */
  key: string;
  order: string;
  listing: string;
  item: string;
  buyer: string;
  envelope: string;
  at: number;
}

/** A purchase this market placed. */
interface Purchase {
  order: string;
  listing: string;
  item: string;
  seller: string;
  at: number;
}

type Tab = "shop" | "sell" | "orders";
type Opened =
  | { kind: "sale"; sale: Sale }
  | { kind: "purchase"; purchase: Purchase };
const TABS: readonly Tab[] = ["shop", "sell", "orders"];

/** The market's account index, which its receiving key is bound to. */
const MARKET_INDEX = toHex(blake2b256(utf8("small-market/account/v1")));
const LISTINGS_KEY = "small-market:own-listings";
const SALES_KEY = "small-market:sales";
const PURCHASES_KEY = "small-market:purchases";
/** The most entries of each kind this market keeps. */
const MAX_KEPT = 50;
const FIELDS = [
  { id: "ship-to", kind: "address" as const },
  { id: "phone", kind: "phone" as const, optional: true },
];

const orderCode = (id: string): string =>
  `#${id.replace(/-/g, "").slice(0, 8).toUpperCase()}`;
const placedAt = (at: number): string =>
  new Date(at).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
const ago = (at: number): string => {
  const minutes = Math.floor((Date.now() - at) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} h ago` : new Date(at).toLocaleDateString();
};
const initial = (item: string): string =>
  ([...item.trim()][0] ?? "?").toUpperCase();
const plural = (count: number, word: string): string =>
  `${count} ${word}${count === 1 ? "" : "s"}`;
const shortAccount = (account: string) =>
  `${account.slice(0, 6)}…${account.slice(-4)}`;

function tile(item: string, size: "cover" | "small" | "hero"): HTMLElement {
  const made = node("div", `tile ${size}`, initial(item));
  paint(made, item);
  made.setAttribute("aria-hidden", "true");
  return made;
}

function empty(
  title: string,
  text: string,
  cta?: { label: string; run(): void },
): HTMLElement {
  const box = node("div", "empty");
  box.append(node("strong", "", title), node("span", "", text));
  if (cta !== undefined) {
    const button = node("button", "secondary", cta.label);
    button.type = "button";
    button.onclick = cta.run;
    box.append(button);
  }
  return box;
}

function withIcon(
  className: string,
  name: Parameters<typeof icon>[0],
  text: string,
): HTMLElement {
  const label = node("span", `${className} with-icon`);
  label.innerHTML = icon(name);
  label.append(text);
  return label;
}

/** A tappable list row: tile, title, subtitle and an optional status chip. */
function row(
  item: string,
  sub: string,
  chip: { text: string; tone: "accent" | "plain" } | null,
  open: () => void,
): HTMLElement {
  const made = node("button", "row");
  made.type = "button";
  const main = node("span", "row-main");
  main.append(node("span", "row-title", item), node("span", "row-sub", sub));
  made.append(tile(item, "small"), main);
  if (chip !== null) {
    const status = node("span", "chip", chip.text);
    status.dataset.tone = chip.tone;
    made.append(status);
  }
  const chevron = node("span", "chev");
  chevron.innerHTML = icon("chevron");
  made.append(chevron);
  made.onclick = open;
  return made;
}

function list(rows: HTMLElement[]): HTMLElement {
  const box = node("div", "list");
  box.append(...rows);
  return box;
}

/** Keep the newest `MAX_KEPT` of `items`, by `at`. */
const bounded = <T extends { at: number }>(items: T[]): T[] =>
  [...items].sort((left, right) => right.at - left.at).slice(0, MAX_KEPT);

fillIcons();
const toast = createToast(element("toast"));
const listButton = element<HTMLButtonElement>("list");
const buyButton = element<HTMLButtonElement>("buy");
const printButton = element<HTMLButtonElement>("print");
const itemInput = element<HTMLInputElement>("item");
const searchInput = element<HTMLInputElement>("search");
const itemScreen = element("item-screen");
const orderScreen = element("order-screen");
const board = element("board");

/** A cancelled request is the user's choice in a host sheet, not a failure. */
function report(error: unknown): void {
  const text =
    error instanceof Error ? error.message : "That didn't work. Try again.";
  toast(
    text,
    error instanceof FieldError && error.code === "cancelled"
      ? "info"
      : "error",
  );
}

function skeleton(): void {
  board.replaceChildren(
    ...Array.from({ length: 4 }, () => {
      const card = node("div", "card skeleton");
      card.append(
        node("div", "tile cover"),
        node("span", "bar"),
        node("span", "bar short"),
      );
      return card;
    }),
  );
}

skeleton();

/** Tabs work at once, while the market still connects. */
function select(next: Tab): void {
  for (const name of TABS) {
    element(`tab-${name}`).setAttribute("aria-selected", String(name === next));
    element(name).hidden = name !== next;
  }
  window.scrollTo(0, 0);
}
for (const name of TABS) element(`tab-${name}`).onclick = () => select(name);

try {
  const client = getClientSync();
  if (client === null) throw new Error("Open Small Market inside the host.");
  const fields = await connect("consumer");
  const storage = await ProductStorage.load(
    client,
    [LISTINGS_KEY, SALES_KEY, PURCHASES_KEY],
    (reason) => toast(`Not saved: ${reason}`, "error"),
  );
  const read = <T>(key: string): T[] => {
    try {
      const value: unknown = JSON.parse(storage.getItem(key) ?? "[]");
      return Array.isArray(value) ? (value as T[]) : [];
    } catch {
      return [];
    }
  };
  const me: ReceiveKey = await fields.createReceiveKey({ index: MARKET_INDEX });
  let own = read<Listing>(LISTINGS_KEY);
  let sales = read<Sale>(SALES_KEY);
  let purchases = read<Purchase>(PURCHASES_KEY);
  /** Others' listings, by id, until they expire. The first copy of an id wins. */
  const listings = new Map<
    string,
    { listing: Listing; expires: number; at: number }
  >();
  /** Fields this market holds for a purchase it placed in this document. */
  const shared = new Map<string, FieldDescriptor[]>();
  let busy = false;
  let openItem: Listing | null = null;
  let opened: Opened | null = null;
  let mounted: ProtectedField[] = [];
  let openHandles: string[] = [];
  const seen = new Set<string>();

  const hear = ({ signer, message, expires }: Heard) => {
    const now = Date.now();
    if (message.type === "listing") {
      if (message.seller.account === me.account || listings.has(message.id))
        return;
      if (listings.size >= MAX_KEPT) {
        const oldest = [...listings].reduce((first, next) =>
          next[1].expires <= now || next[1].at < first[1].at ? next : first,
        );
        listings.delete(oldest[0]);
      }
      listings.set(message.id, { listing: message, expires, at: now });
      render();
      return;
    }
    // Orders for this market's own listings only; anything else is not ours.
    const listing = own.find((each) => each.id === message.listing);
    const key = `${message.id}:${signer}`;
    if (listing === undefined || sales.some((sale) => sale.key === key)) return;
    sales = bounded([
      ...sales,
      {
        key,
        order: message.id,
        listing: listing.id,
        item: listing.item,
        buyer: signer,
        envelope: message.envelope,
        at: now,
      },
    ]);
    storage.setItem(SALES_KEY, JSON.stringify(sales));
    toast(`New order: ${listing.item}`, "success");
    render();
  };
  const network = openBoard(client, hear, (reason) =>
    toast(`Board: ${reason}`, "error"),
  );

  const syncButtons = (): void => {
    listButton.disabled = busy || itemInput.value.trim() === "";
  };

  /** Run one host request at a time; `waiting` relabels the button meanwhile. */
  const action = async (
    task: () => Promise<void>,
    button?: HTMLButtonElement,
    waiting = "Confirm in your host…",
  ): Promise<void> => {
    if (busy) return;
    busy = true;
    const label = button?.innerHTML;
    if (button !== undefined) {
      button.disabled = true;
      button.textContent = waiting;
    }
    syncButtons();
    try {
      await task();
    } catch (error) {
      report(error);
    } finally {
      busy = false;
      if (button !== undefined && label !== undefined) {
        button.innerHTML = label;
        button.disabled = false;
      }
      syncButtons();
    }
  };

  const showScreen = (screen: HTMLElement): void => {
    screen.hidden = false;
    screen.scrollTop = 0;
  };

  const live = () => {
    const now = Date.now();
    return [...listings.values()]
      .filter((kept) => kept.expires > now)
      .map((kept) => kept.listing);
  };

  const showItem = (listing: Listing): void => {
    openItem = listing;
    const mine = listing.seller.account === me.account;
    element("item-tile").replaceWith(
      Object.assign(tile(listing.item, "hero"), { id: "item-tile" }),
    );
    element("item-title").textContent = listing.item;
    buyButton.hidden = mine;
    element("own-note").hidden = !mine;
    showScreen(itemScreen);
  };

  const closeItem = (): void => {
    openItem = null;
    itemScreen.hidden = true;
  };

  const renderShop = (): void => {
    const query = searchInput.value.trim().toLowerCase();
    const all = live().reverse();
    const visible = all.filter((listing) =>
      listing.item.toLowerCase().includes(query),
    );
    element("shop-count").textContent =
      all.length === 0 ? "" : plural(visible.length, "item");
    board.replaceChildren();
    board.classList.toggle("grid", visible.length > 0);
    for (const listing of visible) {
      const card = node("button", "card");
      card.type = "button";
      card.setAttribute("aria-label", `${listing.item}, free`);
      const body = node("span", "card-body");
      body.append(
        node("span", "price", "Free"),
        node("span", "card-title", listing.item),
        withIcon(
          "card-meta",
          "lock",
          `Seller ${shortAccount(listing.seller.account)}`,
        ),
      );
      card.append(tile(listing.item, "cover"), body);
      card.onclick = () => showItem(listing);
      board.append(card);
    }
    if (all.length === 0)
      board.append(
        empty(
          "Nothing for sale yet",
          "New listings show up here as soon as sellers post them.",
          { label: "Sell something", run: () => select("sell") },
        ),
      );
    else if (visible.length === 0)
      board.append(
        empty(
          "No matches",
          `Nothing called “${searchInput.value.trim()}” yet.`,
        ),
      );
  };

  const renderSell = (): void => {
    element("mine").replaceChildren(
      own.length === 0
        ? empty(
            "No listings yet",
            "Name your item above and list it. It's free.",
          )
        : list(
            [...own].reverse().map((listing) => {
              const count = sales.filter(
                (sale) => sale.listing === listing.id,
              ).length;
              return row(
                listing.item,
                count === 0
                  ? "Listed · no orders yet"
                  : `Listed · ${plural(count, "order")}`,
                null,
                () => showItem(listing),
              );
            }),
          ),
    );
  };

  const renderOrders = (): void => {
    element("to-ship").replaceChildren(
      sales.length === 0
        ? empty(
            "Nothing to ship",
            "When someone buys from you, the order shows up here.",
          )
        : list(
            sales.map((sale) =>
              row(
                sale.item,
                `${orderCode(sale.order)} · ${ago(sale.at)}`,
                seen.has(sale.key)
                  ? { text: "To ship", tone: "plain" }
                  : { text: "New", tone: "accent" },
                () => void showOrder({ kind: "sale", sale }),
              ),
            ),
          ),
    );
    element("bought").replaceChildren(
      purchases.length === 0
        ? empty("No purchases", "Things you buy show up here.")
        : list(
            purchases.map((purchase) =>
              row(
                purchase.item,
                `Ordered ${ago(purchase.at)} · ${orderCode(purchase.order)}`,
                { text: "Ordered", tone: "plain" },
                () => void showOrder({ kind: "purchase", purchase }),
              ),
            ),
          ),
    );
    const unseen = sales.filter((sale) => !seen.has(sale.key)).length;
    const badge = element("orders-badge");
    badge.hidden = unseen === 0;
    badge.textContent = String(unseen);
    badge.setAttribute("aria-label", `${plural(unseen, "new order")}`);
  };

  function render(): void {
    renderShop();
    renderSell();
    renderOrders();
  }

  const net = element("net");
  net.textContent = "Statement Store board";
  net.title = "Listings and orders travel as statements this app posts.";
  const avatar = element("me");
  avatar.innerHTML = icon("person");
  avatar.title = `Your seller account ${me.account}`;
  paint(avatar, me.account);

  /** Remove drawn fields and give back the handles of the open order. */
  const clearOrder = (): void => {
    for (const field of mounted) field.remove();
    mounted = [];
    if (openHandles.length > 0)
      void fields.release({ handles: openHandles }).catch(() => {});
    openHandles = [];
    element("slip-address").replaceChildren();
    element("slip-phone").replaceChildren();
  };

  /** Draw `descriptors` in the slip. Answers false when there is no address. */
  const draw = async (descriptors: FieldDescriptor[]): Promise<boolean> => {
    const address = descriptors.find((field) => field.kind === "address");
    const phone = descriptors.find((field) => field.kind === "phone");
    element("slip-phone-block").hidden = phone === undefined;
    if (address === undefined) return false;
    mounted.push(
      await mountProtectedField(
        element("slip-address"),
        fields,
        address.handle,
        {
          title: "Shipping address",
        },
      ),
    );
    if (phone !== undefined)
      mounted.push(
        await mountProtectedField(element("slip-phone"), fields, phone.handle, {
          title: "Phone",
        }),
      );
    return true;
  };

  const showOrder = async (next: Opened): Promise<void> => {
    clearOrder();
    opened = next;
    const sale = next.kind === "sale";
    const order = sale ? next.sale.order : next.purchase.order;
    const item = sale ? next.sale.item : next.purchase.item;
    if (sale) seen.add(next.sale.key);
    element("order-title").textContent = sale ? "Packing slip" : "Receipt";
    const status = element("order-status");
    status.dataset.kind = next.kind;
    element("status-icon").innerHTML = icon(sale ? "box" : "check");
    element("status-title").textContent = sale
      ? "Ready to ship"
      : "Order placed";
    element("status-text").textContent = sale
      ? "Print the label, pack the item and send it."
      : "The seller ships it to the address you chose.";
    element("slip-kind").textContent = sale ? "Packing slip" : "Receipt";
    element("slip-code").textContent = orderCode(order);
    element("slip-date").textContent = placedAt(
      sale ? next.sale.at : next.purchase.at,
    );
    element("slip-thumb").replaceWith(
      Object.assign(tile(item, "small"), { id: "slip-thumb" }),
    );
    element("slip-item").textContent = item;
    element("slip-to-label").textContent = sale ? "Ship to" : "Shipping to";
    element("slip-foot").textContent = sale
      ? "Pack with care. Thank you for selling on Small Market."
      : "Thank you for shopping small.";
    element("trust-text").textContent = sale
      ? "Your Polkadot host shows these details. Small Market can't read them."
      : "Your Polkadot host shows your details. Small Market never received them.";
    const note = element("order-note");
    note.hidden = true;
    printButton.hidden = !sale;
    element("slip-phone-block").hidden = true;
    const slot = element("slip-address");
    slot.replaceChildren(node("div", "address-wait"));
    showScreen(orderScreen);
    renderOrders();
    try {
      let descriptors: FieldDescriptor[] | undefined;
      if (sale) {
        const result = await fields.open({
          envelope: next.sale.envelope,
          context: orderContext(next.sale.listing, next.sale.order),
        });
        descriptors = result.fields;
        openHandles = descriptors.map((field) => field.handle);
      } else descriptors = shared.get(next.purchase.order);
      if (opened !== next) return;
      if (descriptors === undefined || !(await draw(descriptors))) {
        slot.replaceChildren(
          node(
            "p",
            "fine",
            "Your host shared your details with the seller. This app keeps no copy to show after a reload.",
          ),
        );
      }
    } catch (error) {
      slot.replaceChildren(
        node(
          "p",
          "fine",
          error instanceof Error ? error.message : "Details unavailable.",
        ),
      );
    }
  };

  element("item-back").onclick = closeItem;
  element("back").onclick = () => {
    opened = null;
    clearOrder();
    orderScreen.hidden = true;
  };
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    if (!orderScreen.hidden) element("back").click();
    else if (!itemScreen.hidden) closeItem();
  });

  buyButton.onclick = () =>
    void action(async () => {
      const listing = openItem;
      if (listing === null) return;
      if ((listings.get(listing.id)?.expires ?? 0) <= Date.now())
        throw new Error(
          "This listing expired. Ask the seller to list it again.",
        );
      const order = crypto.randomUUID();
      const grant = await fields.request({
        fields: FIELDS,
        purpose: `Ship “${listing.item}” to you`,
        context: orderContext(listing.id, order),
        recipient: listing.seller,
      });
      try {
        if (grant.envelope === undefined)
          throw new Error("The host sealed nothing.");
        await network.post({
          v: 1,
          type: "order",
          id: order,
          listing: listing.id,
          envelope: grant.envelope,
        });
      } catch (error) {
        void fields
          .release({ handles: grant.fields.map((field) => field.handle) })
          .catch(() => {});
        throw error;
      }
      shared.set(order, grant.fields);
      const purchase: Purchase = {
        order,
        listing: listing.id,
        item: listing.item,
        seller: listing.seller.account,
        at: Date.now(),
      };
      purchases = bounded([...purchases, purchase]);
      storage.setItem(PURCHASES_KEY, JSON.stringify(purchases));
      closeItem();
      select("orders");
      await showOrder({ kind: "purchase", purchase });
    }, buyButton);

  printButton.onclick = () =>
    void action(
      async () => {
        if (opened?.kind !== "sale" || openHandles.length === 0) return;
        await fields.print({
          title: "Shipping label",
          lines: [orderCode(opened.sale.order), `1 × ${opened.sale.item}`],
          handles: openHandles,
        });
        toast("Label sent to your printer", "success");
      },
      printButton,
      "Preparing label…",
    );

  searchInput.oninput = renderShop;

  const preview = (): void => {
    const text = itemInput.value.trim();
    element("preview-title").textContent = text === "" ? "Your item" : text;
    element("preview-tile").replaceWith(
      Object.assign(tile(text === "" ? "?" : text, "small"), {
        id: "preview-tile",
      }),
    );
    syncButtons();
  };
  itemInput.oninput = preview;
  preview();

  element<HTMLFormElement>("listing").onsubmit = (event) => {
    event.preventDefault();
    void action(async () => {
      const listing: Listing = {
        v: 1,
        type: "listing",
        id: crypto.randomUUID(),
        item: itemInput.value.trim(),
        seller: me,
      };
      await network.post(listing);
      own = [...own, listing].slice(-MAX_KEPT);
      storage.setItem(LISTINGS_KEY, JSON.stringify(own));
      itemInput.value = "";
      preview();
      toast("Listed. Buyers can see it now.", "success");
      render();
    }, listButton);
  };

  render();
  // Ages and expiries move on without new messages.
  setInterval(() => {
    if (!busy) renderShop();
  }, 5_000);
} catch (error) {
  element("net").textContent = "Not connected";
  element("shop-count").textContent = "";
  board.classList.remove("grid");
  board.replaceChildren(
    empty(
      "Open Small Market in your Polkadot host",
      error instanceof Error ? error.message : "Connection failed.",
    ),
  );
}
