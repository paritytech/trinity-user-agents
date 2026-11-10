import { connectFields, element, FieldError } from "./client.js";
import { createToast, fillIcons, icon, node, paint } from "./ui.js";
import type { FieldNetwork, Listing } from "../../bridges/fields/protocol.js";

interface Order {
  id: string;
  listing: string;
  item: string;
  at: number;
  buyer?: string;
  seller?: string;
}

interface Snapshot {
  network: FieldNetwork;
  account: string;
  listings: Listing[];
  orders: Order[];
  purchases: Order[];
}

type Tab = "shop" | "sell" | "orders";
type OrderKind = "sale" | "purchase";
const TABS: readonly Tab[] = ["shop", "sell", "orders"];

/** The marketplace operations of the host's demo bridge, typed in one place. */
function marketApi(
  call: (request: Record<string, unknown>) => Promise<unknown>,
) {
  return {
    refresh: () => call({ op: "refresh" }) as Promise<Snapshot>,
    list: (item: string) => call({ op: "list", item }),
    buy: async (listing: string) =>
      ((await call({ op: "buy", listing })) as { order: string }).order,
    addressView: async (order: string) =>
      (
        (await call({ op: "address", order, theme: "paper" })) as {
          view: string;
        }
      ).view,
    print: (order: string) => call({ op: "print", order }),
  };
}

/** What the network pill says. Simulation is always named as such. */
function network(info: FieldNetwork): { label: string; simulated: boolean } {
  return info.kind === "simulated"
    ? { label: "Simulated network", simulated: true }
    : { label: `Live · ${info.name}`, simulated: false };
}

const orderCode = (id: string): string =>
  `#${id.replace(/-/g, "").slice(0, 8).toUpperCase()}`;
const placedAt = (at: number): string =>
  at === 0
    ? "Earlier"
    : new Date(at).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
      });
const ago = (at: number): string => {
  if (at === 0) return "earlier";
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

try {
  const market = marketApi(await connectFields("market"));
  let snapshot: Snapshot | null = null;
  let shown = "";
  let busy = false;
  let openItem: Listing | null = null;
  let openOrder: { kind: OrderKind; order: Order } | null = null;
  const seen = new Set<string>();

  const syncButtons = (): void => {
    listButton.disabled = busy || itemInput.value.trim() === "";
  };

  /** Run one host request at a time; `waiting` relabels the button while the host asks. */
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

  const select = (next: Tab): void => {
    for (const name of TABS) {
      element(`tab-${name}`).setAttribute(
        "aria-selected",
        String(name === next),
      );
      element(name).hidden = name !== next;
    }
    window.scrollTo(0, 0);
  };
  for (const name of TABS) element(`tab-${name}`).onclick = () => select(name);

  const showScreen = (screen: HTMLElement): void => {
    screen.hidden = false;
    screen.scrollTop = 0;
  };

  const showItem = (listing: Listing): void => {
    openItem = listing;
    const own = listing.seller === snapshot?.account;
    element("item-tile").replaceWith(
      Object.assign(tile(listing.item, "hero"), { id: "item-tile" }),
    );
    element("item-title").textContent = listing.item;
    buyButton.hidden = own;
    element("own-note").hidden = !own;
    showScreen(itemScreen);
  };

  const closeItem = (): void => {
    openItem = null;
    itemScreen.hidden = true;
  };

  const renderShop = (data: Snapshot): void => {
    const query = searchInput.value.trim().toLowerCase();
    const listings = [...data.listings].reverse();
    const visible = listings.filter((listing) =>
      listing.item.toLowerCase().includes(query),
    );
    element("shop-count").textContent =
      listings.length === 0 ? "" : plural(visible.length, "item");
    board.replaceChildren();
    board.classList.toggle("grid", visible.length > 0);
    for (const listing of visible) {
      const own = listing.seller === data.account;
      const card = node("button", "card");
      card.type = "button";
      card.setAttribute("aria-label", `${listing.item}, free`);
      const body = node("span", "card-body");
      body.append(
        node("span", "price", "Free"),
        node("span", "card-title", listing.item),
        own
          ? node("span", "card-meta own", "Your listing")
          : withIcon("card-meta", "lock", "Private delivery"),
      );
      card.append(tile(listing.item, "cover"), body);
      card.onclick = () => showItem(listing);
      board.append(card);
    }
    if (listings.length === 0)
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

  const renderSell = (data: Snapshot): void => {
    const own = [...data.listings]
      .reverse()
      .filter((listing) => listing.seller === data.account);
    element("mine").replaceChildren(
      own.length === 0
        ? empty(
            "No listings yet",
            "Name your item above and list it. It's free.",
          )
        : list(
            own.map((listing) => {
              const count = data.orders.filter(
                (order) => order.listing === listing.id,
              ).length;
              return row(
                listing.item,
                count === 0
                  ? "Live · no orders yet"
                  : `Live · ${plural(count, "order")}`,
                null,
                () => showItem(listing),
              );
            }),
          ),
    );
  };

  const renderOrders = (data: Snapshot): void => {
    element("to-ship").replaceChildren(
      data.orders.length === 0
        ? empty(
            "Nothing to ship",
            "When someone buys from you, the order shows up here.",
          )
        : list(
            data.orders.map((order) =>
              row(
                order.item,
                `${orderCode(order.id)} · ${ago(order.at)}`,
                seen.has(order.id)
                  ? { text: "To ship", tone: "plain" }
                  : { text: "New", tone: "accent" },
                () => void showOrder("sale", order),
              ),
            ),
          ),
    );
    element("bought").replaceChildren(
      data.purchases.length === 0
        ? empty("No purchases", "Things you buy show up here.")
        : list(
            data.purchases.map((order) =>
              row(
                order.item,
                `Ordered ${ago(order.at)} · ${orderCode(order.id)}`,
                { text: "Ordered", tone: "plain" },
                () => void showOrder("purchase", order),
              ),
            ),
          ),
    );
    const unseen = data.orders.filter((order) => !seen.has(order.id)).length;
    const badge = element("orders-badge");
    badge.hidden = unseen === 0;
    badge.textContent = String(unseen);
    badge.setAttribute("aria-label", `${plural(unseen, "new order")}`);
  };

  const render = (data: Snapshot): void => {
    const net = element("net");
    const { label, simulated } = network(data.network);
    net.textContent = label;
    net.title = simulated
      ? "Simulated Statement Store. Nothing is posted to a chain."
      : `Posts to the ${data.network.name} Statement Store.`;
    net.toggleAttribute("data-simulated", simulated);
    const me = element("me");
    me.innerHTML = icon("person");
    paint(me, data.account);
    renderShop(data);
    renderSell(data);
    renderOrders(data);
  };

  const update = async (): Promise<void> => {
    const next = await market.refresh();
    const before = snapshot;
    snapshot = next;
    const text = JSON.stringify(next);
    if (text === shown) return;
    shown = text;
    if (before !== null)
      for (const order of next.orders)
        if (!before.orders.some((known) => known.id === order.id))
          toast(`New order: ${order.item}`, "success");
    render(next);
  };

  const showOrder = async (kind: OrderKind, order: Order): Promise<void> => {
    openOrder = { kind, order };
    seen.add(order.id);
    const sale = kind === "sale";
    element("order-title").textContent = sale ? "Packing slip" : "Receipt";
    const status = element("order-status");
    status.dataset.kind = kind;
    element("status-icon").innerHTML = icon(sale ? "box" : "check");
    element("status-title").textContent = sale
      ? "Ready to ship"
      : "Order placed";
    element("status-text").textContent = sale
      ? "Print the label, pack the item and send it."
      : "The seller ships it to the address you chose.";
    element("slip-kind").textContent = sale ? "Packing slip" : "Receipt";
    element("slip-code").textContent = orderCode(order.id);
    element("slip-date").textContent = placedAt(order.at);
    element("slip-thumb").replaceWith(
      Object.assign(tile(order.item, "small"), { id: "slip-thumb" }),
    );
    element("slip-item").textContent = order.item;
    element("slip-to-label").textContent = sale ? "Ship to" : "Shipping to";
    element("slip-foot").textContent = sale
      ? "Pack with care. Thank you for selling on Small Market."
      : "Thank you for shopping small.";
    element("trust-text").textContent = sale
      ? "Your Polkadot host shows this address. Small Market can't read it."
      : "Your Polkadot host shows your address. Small Market never received it.";
    printButton.hidden = !sale;
    const slot = element("slip-address");
    slot.replaceChildren(node("div", "address-wait"));
    showScreen(orderScreen);
    if (snapshot !== null) renderOrders(snapshot);
    try {
      const view = await market.addressView(order.id);
      if (openOrder?.order.id !== order.id) return;
      const frame = node("iframe", "host-field");
      frame.title = sale ? "Ship-to address" : "Shipping address";
      frame.referrerPolicy = "no-referrer";
      frame.src = view;
      slot.replaceChildren(frame);
    } catch (error) {
      slot.replaceChildren(
        node(
          "p",
          "fine",
          error instanceof Error
            ? error.message
            : "Address unavailable. Reopen the order.",
        ),
      );
    }
  };

  element("item-back").onclick = closeItem;
  element("back").onclick = () => {
    openOrder = null;
    orderScreen.hidden = true;
    element("slip-address").replaceChildren();
  };
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    if (!orderScreen.hidden) element("back").click();
    else if (!itemScreen.hidden) closeItem();
  });

  buyButton.onclick = () =>
    void action(async () => {
      if (openItem === null) return;
      const id = await market.buy(openItem.id);
      await update();
      const placed = snapshot?.purchases.find((each) => each.id === id);
      closeItem();
      select("orders");
      if (placed !== undefined) await showOrder("purchase", placed);
    }, buyButton);

  printButton.onclick = () =>
    void action(
      async () => {
        if (openOrder?.kind !== "sale") return;
        await market.print(openOrder.order.id);
        toast("Label sent to your printer", "success");
      },
      printButton,
      "Preparing label…",
    );

  searchInput.oninput = () => {
    if (snapshot !== null) renderShop(snapshot);
  };

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
      await market.list(itemInput.value);
      itemInput.value = "";
      preview();
      toast("Listed. Buyers can see it now.", "success");
      await update();
    }, listButton);
  };

  await action(update);
  setInterval(() => {
    if (!busy) void update().catch(() => {});
  }, 2_000);
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
