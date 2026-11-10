import type { PromptRequest } from "../app/prompt.js";
import {
  cryptoRandom,
  fromHex,
  fromUtf8,
  toHex,
  utf8,
} from "../../bridges/primitives/bytes.js";
import {
  digest,
  openSealed,
  sealTo,
  x25519Pair,
  type X25519Pair,
} from "../invitations/sealing.js";
import {
  FIELD_DEMO,
  FieldError,
  type Field,
  type FieldErrorReply,
  type FieldNetwork,
  type ProviderStatus,
  type Listing,
  type Purchase,
  type Received,
} from "../../bridges/fields/protocol.js";
import type { FieldTransport } from "./transport.js";

interface Provider {
  /** The name the provider gives itself. Shown beside its verified product id, never alone. */
  name: string;
  fields: Field[];
  values: Partial<Record<Field, string>>;
}

/** How a host-rendered field view is drawn. The product picks one of these, never CSS. */
export type ViewTheme = "paper" | "dark";

/** A shipping label the host composes from its own records and prints itself. */
export interface ShippingLabel {
  order: string;
  item: string;
  product: string;
  address: string[];
  at: number;
}

/** Dependencies of the web-only, in-memory field-provider experiment. */
export interface FieldHostOptions {
  ask<T>(request: PromptRequest<T>): Promise<T>;
  open(
    product: string,
    receive: (message: Received) => void,
    signal: AbortSignal,
  ): Promise<FieldTransport>;
  /** The host-origin page that renders one single-use view token. */
  viewUrl(token: string, theme: ViewTheme): string;
  /**
   * Print a label from a host document. Products cannot print: their frame
   * has no `allow-modals`. Settles once the host document is gone; `signal`
   * aborts it, and removes the document, when the product or session ends.
   */
  print(label: ShippingLabel, signal: AbortSignal): Promise<void>;
  now?(): number;
}

/** One live product connection: what to revoke when it ends. */
interface Connection {
  /** The window of the product frame the bridge port came from. */
  frame(): unknown;
  /** Ports of host views drawn inside this product, blanked on close. */
  views: Set<MessagePort>;
  /** Unused view tokens issued to this product. */
  tokens: Set<string>;
}

interface PendingView {
  owner: Connection;
  expires: number;
  text(): string | null;
}

/** A public message a connection keeps: when it arrived and when its statement expires. */
interface Kept<T> {
  value: T;
  at: number;
  expires: number;
}

/** A listing this session published, and the product it published it for. */
interface OwnListing {
  product: string;
  listing: Listing;
}

/** An order this host sent, kept in host memory for the buyer's own views. */
interface SentOrder {
  product: string;
  listing: Listing;
  address: string;
  at: number;
}

/** How long an issued view token waits for its frame. */
const VIEW_TOKEN_MS = 60_000;
/** Bounded state per session and connection. */
const MAX_ITEMS = 50;
const MAX_LINES = 6;

/** How host sheets describe the network a post goes to. */
const networkText = (network: FieldNetwork): string =>
  network.kind === "simulated"
    ? "SIMULATED Statement Store. Nothing leaves this browser and no allowance is used."
    : `Public on the ${network.name} Statement Store. May use one of this account's daily posting slots. No payments.`;

/** A provider's self-declared name: printable, trimmed, 1 to 40 characters. */
const providerName = (value: unknown): string | null => {
  if (typeof value !== "string") return null;
  const name = value.replace(/[\p{C}]/gu, "").trim();
  return name.length > 0 && name.length <= 40 ? name : null;
};

/**
 * A failure as the product sees it. Only {@link FieldError}s keep their text;
 * anything else is reported generically, so no unexpected message crosses.
 */
const errorReply = (error: unknown): FieldErrorReply =>
  error instanceof FieldError
    ? { code: error.code, message: error.message }
    : { code: "unavailable", message: "Request failed." };

/**
 * Keep `entry` under `id`, unless an entry is already there: the first copy of
 * an id wins, so a later duplicate never replaces it. A full map makes room by
 * dropping what has expired, or else what arrived first, never the new entry.
 * Entries `kept` names are never dropped and do not count.
 */
function keep<T>(
  map: Map<string, Kept<T>>,
  id: string,
  entry: Kept<T>,
  now: number,
  kept: (id: string) => boolean = () => false,
): void {
  if (map.has(id)) return;
  const droppable = [...map].filter(([key]) => !kept(key));
  if (droppable.length >= MAX_ITEMS) {
    const expired = droppable.filter(([, item]) => item.expires <= now);
    for (const [key] of expired) map.delete(key);
    if (expired.length === 0)
      map.delete(
        droppable.reduce((oldest, next) =>
          next[1].at < oldest[1].at ? next : oldest,
        )[0],
      );
  }
  map.set(id, entry);
}

/** Address lines as a host view shows them: trimmed and non-empty. */
export function addressLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** Whether `source` is a window whose parent is exactly `frame`. */
const isChildOf = (source: unknown, frame: unknown): boolean => {
  if (typeof source !== "object" || source === null || frame === null)
    return false;
  try {
    return (source as { parent?: unknown }).parent === frame;
  } catch {
    return false;
  }
};

/**
 * Host-owned field declarations, selection, consent, keys, and the views the
 * host draws inside a product. A product gets a single-use token bound to its
 * frame; only a host-origin frame that is a direct child of that product frame
 * can redeem it, and its text never crosses the product's bridge.
 */
export class FieldHost {
  private scope: string | null = null;
  private readonly providers = new Map<string, Provider>();
  private readonly keys = new Map<string, X25519Pair>();
  private readonly connections = new Set<() => void>();
  private readonly views = new Map<string, PendingView>();
  private readonly sent = new Map<string, SentOrder>();
  /** Listings this session published, by id: the only ones whose orders it keeps. */
  private readonly own = new Map<string, OwnListing>();
  private printing = false;

  constructor(private readonly options: FieldHostOptions) {}

  private now(): number {
    return this.options.now?.() ?? Date.now();
  }

  /** End every pending request and drop plaintext, keys and views when the session changes. */
  setScope(scope: string | null): void {
    if (scope === this.scope) return;
    for (const close of [...this.connections]) close();
    this.providers.clear();
    for (const key of this.keys.values()) key.secret.fill(0);
    this.keys.clear();
    this.views.clear();
    this.sent.clear();
    this.own.clear();
    this.scope = scope;
  }

  /**
   * Redeem a view token for a host-origin frame. Answers `{ lines }` only when
   * the token is live and `source` is a direct child of the frame the token
   * was issued to; anything else gets `{ unavailable: true }`.
   */
  attachView(token: unknown, source: unknown, port: MessagePort): void {
    const view = typeof token === "string" ? this.views.get(token) : undefined;
    if (view !== undefined) {
      this.views.delete(token as string);
      view.owner.tokens.delete(token as string);
    }
    const text =
      view !== undefined &&
      view.expires >= this.now() &&
      isChildOf(source, view.owner.frame())
        ? view.text()
        : null;
    if (view === undefined || text === null) {
      port.postMessage({ unavailable: true });
      port.close();
      return;
    }
    port.postMessage({ lines: addressLines(text) });
    const owner = view.owner;
    owner.views.add(port);
    // The view says when its document goes away, so drawn views do not pile up.
    port.onmessage = (event: MessageEvent<unknown>) => {
      if ((event.data as { release?: unknown } | null)?.release !== true)
        return;
      owner.views.delete(port);
      port.close();
    };
  }

  /** Attach an already source- and origin-checked product port, with its declared role. */
  connect(
    product: string,
    role: "provider" | "market",
    port: MessagePort,
    frame: () => unknown = () => null,
  ): () => void {
    const scope = this.scope;
    const controller = new AbortController();
    const connection: Connection = {
      frame,
      views: new Set(),
      tokens: new Set(),
    };
    let network: FieldTransport | null = null;
    let queue = Promise.resolve();
    /** Listings by id, until they expire. */
    const listings = new Map<string, Kept<Listing>>();
    /** Orders for this session's own listings, by purchase id and buyer. */
    const purchases = new Map<string, Kept<Received & { message: Purchase }>>();
    const ownListing = (id: string) => this.own.has(id);
    const purpose = (listing: Listing, buyer: string, purchase: string) =>
      toHex(
        digest(
          FIELD_DEMO,
          listing.id,
          listing.item,
          listing.seller,
          listing.key,
          buyer,
          purchase,
          "address",
        ),
      );
    /** This session's listing `id` for this product, and the key that opens its orders. */
    const mine = (id: string) => {
      const own = this.own.get(id);
      const key =
        own === undefined ? undefined : this.keys.get(own.listing.key);
      return own === undefined || key === undefined || own.product !== product
        ? null
        : { listing: own.listing, key };
    };
    const receive = (received: Received) => {
      if (controller.signal.aborted || scope !== this.scope) return;
      const now = this.now();
      if (received.expires <= now) return;
      const { sender, message } = received;
      if (message.type === "listing") {
        keep(
          listings,
          message.id,
          { value: message, at: now, expires: received.expires },
          now,
          ownListing,
        );
        return;
      }
      // A seller keeps only orders for its own listings that its key opens.
      // Anything else is another seller's order or noise, and takes no room.
      const target = mine(message.listing);
      if (target === null) return;
      const opened = openSealed(
        target.key,
        fromHex(message.box),
        purpose(target.listing, sender, message.id),
      );
      if (opened === null) return;
      opened.fill(0);
      keep(
        purchases,
        `${message.id}:${sender}`,
        {
          value: { ...received, message },
          at: now,
          expires: received.expires,
        },
        now,
      );
    };
    const active = () => {
      if (scope === null || scope !== this.scope || controller.signal.aborted)
        throw new FieldError(
          "session",
          "Session ended. Sign in and reopen the product.",
        );
    };
    const close = () => {
      if (controller.signal.aborted) return;
      controller.abort();
      network?.close();
      port.close();
      for (const view of connection.views) {
        view.postMessage({ clear: true });
        view.close();
      }
      connection.views.clear();
      for (const token of connection.tokens) this.views.delete(token);
      connection.tokens.clear();
      this.connections.delete(close);
    };
    this.connections.add(close);
    const ask = async <T>(request: PromptRequest<T>): Promise<T> => {
      active();
      const result = await this.options.ask({
        ...request,
        from: product,
        signal: controller.signal,
      });
      active();
      return result;
    };
    const net = async () => {
      active();
      if (network === null) {
        const opened = await this.options.open(
          product,
          receive,
          controller.signal,
        );
        if (controller.signal.aborted || scope !== this.scope) {
          opened.close();
          active();
        }
        network = opened;
      }
      network.check();
      return network;
    };
    /** An order this seller's host can open, with the listing it is for. */
    const incoming = (transport: FieldTransport, order: unknown) => {
      const kept = typeof order === "string" ? purchases.get(order) : undefined;
      const target =
        kept === undefined ? null : mine(kept.value.message.listing);
      if (
        kept === undefined ||
        target === null ||
        target.listing.seller !== transport.account
      )
        return null;
      const received = kept.value;
      const { listing, key } = target;
      const open = (): string | null => {
        const opened = openSealed(
          key,
          fromHex(received.message.box),
          purpose(listing, received.sender, received.message.id),
        );
        if (opened === null) return null;
        try {
          return fromUtf8(opened);
        } finally {
          opened.fill(0);
        }
      };
      return { received, listing, open, at: kept.at };
    };
    const issueView = (text: () => string | null, theme: ViewTheme) => {
      const now = this.now();
      for (const token of connection.tokens)
        if ((this.views.get(token)?.expires ?? 0) < now) {
          this.views.delete(token);
          connection.tokens.delete(token);
        }
      if (connection.tokens.size >= MAX_ITEMS)
        throw new FieldError(
          "limit",
          "Too many open views. Reopen the product.",
        );
      const token = toHex(cryptoRandom(16)).slice(2);
      this.views.set(token, {
        owner: connection,
        expires: this.now() + VIEW_TOKEN_MS,
        text,
      });
      connection.tokens.add(token);
      return { view: this.options.viewUrl(token, theme) };
    };
    const call = async (request: Record<string, unknown>): Promise<unknown> => {
      active();
      if (role === "provider") {
        if (request.op === "declare") {
          const fields = request.fields;
          if (
            !Array.isArray(fields) ||
            fields.length === 0 ||
            fields.length > 2 ||
            fields.some((field) => field !== "address" && field !== "phone") ||
            new Set(fields).size !== fields.length
          )
            throw new FieldError(
              "invalid",
              "Declare address and/or phone only.",
            );
          const name = providerName(request.name);
          if (name === null)
            throw new FieldError(
              "invalid",
              "Name the app in 1 to 40 characters.",
            );
          // No sheet: only apps the host activated as providers reach this
          // (`tools/demos/field-demo.ts`), registering discloses nothing and
          // spends nothing, and every later disclosure asks, naming the app.
          const known = this.providers.get(product);
          if (known !== undefined) return { registered: known.fields };
          this.providers.set(product, { name, fields, values: {} });
          return { registered: fields };
        }
        if (request.op === "save") {
          const provider = this.providers.get(product);
          if (provider === undefined)
            throw new FieldError("invalid", "Register declarations first.");
          if (typeof request.values !== "object" || request.values === null)
            throw new FieldError("invalid", "Invalid fields.");
          const values = request.values as Record<string, unknown>;
          const next: Partial<Record<Field, string>> = {};
          for (const [field, value] of Object.entries(values)) {
            if (
              !provider.fields.includes(field as Field) ||
              typeof value !== "string" ||
              value.trim().length === 0 ||
              value.length > 160
            )
              throw new FieldError(
                "invalid",
                "Use declared fields with 1 to 160 characters.",
              );
            if (field === "address" && addressLines(value).length > MAX_LINES)
              throw new FieldError(
                "invalid",
                `Use at most ${MAX_LINES} address lines.`,
              );
            next[field as Field] = value;
          }
          provider.values = next;
          return { saved: Object.keys(next) };
        }
        if (request.op === "status") {
          const provider = this.providers.get(product);
          const status: ProviderStatus = {
            registered: provider !== undefined,
            name: provider?.name ?? null,
            fields: provider?.fields ?? [],
            saved:
              provider === undefined
                ? []
                : provider.fields.filter(
                    (field) => provider.values[field] !== undefined,
                  ),
          };
          return status;
        }
        throw new FieldError(
          "unsupported",
          "Provider operation not supported.",
        );
      }
      const transport = await net();
      if (request.op === "refresh") {
        const now = this.now();
        return {
          network: transport.network,
          account: transport.account,
          listings: [...listings.values()]
            .filter((kept) => kept.expires > now)
            .map((kept) => kept.value),
          orders: [...purchases.keys()]
            .flatMap((id) => {
              const found = incoming(transport, id);
              return found === null
                ? []
                : [
                    {
                      id,
                      listing: found.listing.id,
                      item: found.listing.item,
                      buyer: found.received.sender,
                      at: found.at,
                    },
                  ];
            })
            .sort((left, right) => right.at - left.at),
          purchases: [...this.sent]
            .filter(([, order]) => order.product === product)
            .map(([id, order]) => ({
              id,
              listing: order.listing.id,
              item: order.listing.item,
              seller: order.listing.seller,
              at: order.at,
            }))
            .sort((left, right) => right.at - left.at),
        };
      }
      if (request.op === "list") {
        if (
          typeof request.item !== "string" ||
          request.item.trim().length === 0 ||
          request.item.length > 80
        )
          throw new FieldError(
            "invalid",
            "Use an item name with 1 to 80 characters.",
          );
        if (this.keys.size >= MAX_ITEMS)
          throw new FieldError(
            "limit",
            "Too many listings in this session. Sign out to clear it.",
          );
        const key = x25519Pair(cryptoRandom(32));
        const publicKey = toHex(key.public);
        this.keys.set(publicKey, key);
        const forget = () => {
          key.secret.fill(0);
          this.keys.delete(publicKey);
        };
        const accepted = await ask({
          title: `Publish “${request.item.trim()}”?`,
          subtitle: networkText(transport.network),
          fields: [
            {
              label: "Delivery key · buyers compare this",
              value: publicKey,
              mono: true,
            },
          ],
          details: {
            summary: "Demo limits",
            fields: [
              {
                label: "Pairing",
                value:
                  "Buyers compare this key by hand before they share an address. A signed listing does not prove that only your host holds the key.",
              },
              {
                label: "Kept",
                value:
                  "This tab only. Reload or sign-out loses the key and the orders it opens.",
              },
            ],
          },
          choices: [
            { label: "Publish", value: true },
            { label: "Cancel", value: false, tone: "cancel" },
          ],
          dismissed: false,
        }).catch((error: unknown) => {
          forget();
          throw error;
        });
        if (!accepted) {
          forget();
          throw new FieldError("cancelled", "Listing cancelled.");
        }
        const listing: Listing = {
          v: 1,
          type: "listing",
          id: crypto.randomUUID(),
          item: request.item.trim(),
          seller: transport.account,
          key: publicKey,
        };
        let expires: number;
        try {
          expires = await transport.post(listing);
        } catch (error) {
          forget();
          throw error;
        }
        active();
        this.own.set(listing.id, { product, listing });
        keep(
          listings,
          listing.id,
          { value: listing, at: this.now(), expires },
          this.now(),
          ownListing,
        );
        return { listed: listing.id };
      }
      if (request.op === "buy") {
        const kept =
          typeof request.listing === "string"
            ? listings.get(request.listing)
            : undefined;
        const listing = kept?.value;
        if (listing === undefined || listing.seller === transport.account)
          throw new FieldError(
            "unavailable",
            "Select another account's listing.",
          );
        // Checked again after the sheet: the user may answer after it expired.
        const live = () => {
          if (kept!.expires <= this.now())
            throw new FieldError(
              "unavailable",
              "This listing expired. Ask the seller to list it again.",
            );
        };
        live();
        if (this.sent.size >= MAX_ITEMS)
          throw new FieldError(
            "limit",
            "Too many orders in this session. Sign out to clear it.",
          );
        // Addresses are read once, here: what the sheet shows is what is sealed,
        // even if a provider saves a new value while the sheet is open.
        const choices = [...this.providers].flatMap(([id, provider]) =>
          provider.fields.includes("address") &&
          provider.values.address !== undefined
            ? [
                {
                  id,
                  from: `${provider.name} · ${id}`,
                  address: provider.values.address,
                },
              ]
            : [],
        );
        if (choices.length === 0)
          throw new FieldError(
            "unavailable",
            "No address saved yet. Open a details app, such as Field notebook, and save an address.",
          );
        // One sheet: the address shown is exactly what is sealed, the app it
        // comes from is a choice when there is more than one, and the key
        // comparison is part of the answer.
        const only = choices.length === 1 ? choices[0]! : null;
        const shown = (address: string) => addressLines(address).join(", ");
        const selected = await ask<string | null>({
          title: "Share your address with the seller?",
          subtitle: networkText(transport.network),
          fields: [
            { label: "Item", value: listing.item },
            ...(only === null
              ? []
              : [
                  { label: "Address", value: shown(only.address) },
                  { label: "From", value: only.from },
                ]),
            { label: "Seller's delivery key", value: listing.key, mono: true },
          ],
          details: {
            summary: "Demo limits",
            fields: [
              {
                label: "Pairing",
                value:
                  "Compare the full key with the one the seller's host showed. The listing and its signature do not prove that only the seller's host holds it.",
              },
              {
                label: "Who can read it",
                value:
                  "The provider app, your host and the seller's host. Not the market app, on either side. Phone is not shared. This development host is not a sandbox.",
              },
            ],
          },
          ...(only === null
            ? {
                layout: "list" as const,
                choices: [
                  ...choices.map((choice) => ({
                    label: `Keys match, share ${shown(choice.address)} · from ${choice.from}`,
                    value: choice.id,
                  })),
                  { label: "Cancel", value: null, tone: "cancel" as const },
                ],
              }
            : {
                choices: [
                  { label: "Keys match, share address", value: only.id },
                  { label: "Cancel", value: null, tone: "cancel" as const },
                ],
              }),
          dismissed: null,
        });
        if (selected === null)
          throw new FieldError("cancelled", "Purchase cancelled.");
        live();
        const address = choices.find(
          (choice) => choice.id === selected,
        )?.address;
        if (address === undefined)
          throw new FieldError("unavailable", "Provider unavailable.");
        const id = crypto.randomUUID();
        const box = toHex(
          sealTo(
            fromHex(listing.key),
            utf8(address),
            purpose(listing, transport.account, id),
            cryptoRandom,
          ),
        );
        const purchase: Purchase = {
          v: 1,
          type: "purchase",
          id,
          listing: listing.id,
          box,
        };
        await transport.post(purchase);
        active();
        this.sent.set(id, { product, listing, address, at: this.now() });
        return { order: id };
      }
      if (request.op === "address") {
        const theme: ViewTheme = request.theme === "dark" ? "dark" : "paper";
        const mine =
          typeof request.order === "string"
            ? this.sent.get(request.order)
            : undefined;
        if (mine !== undefined && mine.product === product)
          return issueView(() => mine.address, theme);
        const found = incoming(transport, request.order);
        if (found === null)
          throw new FieldError("unavailable", "No such order in this session.");
        return issueView(found.open, theme);
      }
      if (request.op === "print") {
        const found = incoming(transport, request.order);
        if (found === null)
          throw new FieldError("unavailable", "No such order in this session.");
        if (this.printing)
          throw new FieldError("busy", "A label is already printing.");
        const text = found.open();
        if (text === null)
          throw new FieldError(
            "unavailable",
            "This order could not be opened.",
          );
        this.printing = true;
        try {
          await this.options.print(
            {
              order: found.received.message.id,
              item: found.listing.item,
              product,
              address: addressLines(text),
              at: found.at,
            },
            controller.signal,
          );
        } finally {
          this.printing = false;
        }
        active();
        return { printed: true };
      }
      throw new FieldError(
        "unsupported",
        "Marketplace operation not supported.",
      );
    };
    port.onmessage = (event: MessageEvent<unknown>) => {
      const request = event.data;
      if (typeof request !== "object" || request === null) return;
      const record = request as Record<string, unknown>;
      if (typeof record.id !== "number" || !Number.isSafeInteger(record.id))
        return;
      const id = record.id;
      queue = queue.then(async () => {
        try {
          const result = await call(record);
          active();
          port.postMessage({ id, result });
        } catch (error) {
          if (!controller.signal.aborted)
            port.postMessage({ id, error: errorReply(error) });
        }
      });
    };
    port.start();
    port.postMessage({ protocol: FIELD_DEMO, role });
    return close;
  }
}
