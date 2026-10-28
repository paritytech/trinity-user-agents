import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import {
  displayAddress,
  effectiveProductIdFor,
  parseTypedAddress,
  type Address,
} from "./address.js";
import type { ShelfApp } from "./apps-shelf.js";
import {
  DESTINATIONS,
  DESTINATION_LABELS,
  destinationOf,
  isProductDestination,
  defaultAddresses,
  loadAddresses,
  resolveNavigation,
  type Destination,
  type DockAddresses,
} from "./shell.js";
import { resolveSource } from "../products/dotns.js";
import {
  documentsBeforeProduct,
  type StageLoading,
} from "../products/loading.js";
import {
  loaderUrl,
  mountScope,
  requireSecureHost,
} from "../products/loader/mount.js";
import { openProduct, type OpenProduct } from "../products/product.js";
import { notOpenableText, type Opened } from "../products/status.js";
import type { FrameTap } from "../products/tap.js";
import type { ActivityDetail, ActivitySink } from "../platform/activity.js";
import type { NetworkConfig } from "../platform/network-config.js";
import { tabProductKey } from "../platform/network-choice.js";
import type { PocketCardKey } from "../pocket/faces.js";
import { cardLaunchPath } from "../pocket/pocket.js";
import { resolveWidget } from "../pocket/widget.js";

/** How the sandbox status of a product whose loader failed starts. */
export const SANDBOX_FAILED = "Sandbox failed:";

type Runtime = WorkerPairingHostRuntime;

/** A product held behind the one in front, with the bookkeeping that belongs to it. */
interface Held {
  product: OpenProduct;
  opened: Opened | null;
  entered: boolean;
  origin: string | null;
  sandbox: string;
  domains: string[];
}

/** Where a navigation ends up and how it got there. */
interface Target extends Opened {
  url: URL;
}

/** A product this tab opened itself, kept so a reload brings it back. */
export interface TabProduct {
  address: string;
  productId: string;
  destination: Destination | null;
  parsed: Address;
}

/** What navigation needs from the page around it. */
export interface NavigationDeps {
  network: NetworkConfig;
  /** This host's directory on its origin. Mounted products live under it. */
  base: string;
  /** Where the product in front is embedded. */
  slot: HTMLElement;
  /** The Product tab's status line, which says what an open did. */
  statusLine: HTMLElement;
  loading: StageLoading;
  /** Run a task after every session change and open queued before it. */
  serial(task: (runtime: Runtime) => Promise<void>): Promise<void>;
  /** The signed-in wallet, which mounted content is kept under. */
  walletId(): string | null;
  /** Whether the tab acts as a test user, which opens no product by destination. */
  testUser: boolean;
  tap(runtime: Runtime, productId: string): FrameTap;
  /** Show `text` in the address bar. */
  showAddress(text: string): void;
  /** Empty the address bar, which names no product over a host screen. */
  clearAddress(): void;
  /** Draw everything that depends on the product in front. */
  render(): void;
  /** A held product came back in front. */
  resumed(): void;
  /** An App was opened by its address, for the history. */
  recorded(entry: {
    address: string;
    productId: string;
    entered: boolean;
  }): void;
  activity: ActivitySink;
}

/** Where an app that belongs to no destination is held. */
export const appKey = (productId: string): string => `app:${productId}`;

/** The scope an entered product id was used with, for the log. */
export function scopeOf(address: Address): string {
  return address.kind === "url" ? address.url.origin : address.name;
}

/** A message for an error thrown here, without the `Error:` prefix. */
const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/** How the log names a product: the name it was opened by, else its id. */
const productName = (product: OpenProduct, how: Opened | null): string =>
  how?.via === "name" ? how.address : product.productId;

/**
 * The product in front of the stage and the ones running behind it: which
 * dock destination is shown, what opening a name, a URL, a Pocket card or a
 * product's own navigation request does, and how products survive a session
 * change. Every change runs through {@link NavigationDeps.serial}, so a late
 * result never lands on a product that is no longer in front.
 */
export class ProductNavigation {
  private front: OpenProduct | null = null;
  private how: Opened | null = null;
  /** Whether the open product's id was entered rather than derived. */
  private enteredId = false;
  /** The origin the entered product id was last opened with, for the log. */
  private origin: string | null = null;
  /** What the sandbox loader last said about the open product, until the page takes over. */
  private sandbox = "";
  /** Domains the core asked about for the open product, in this tab. */
  private domains: string[] = [];
  /**
   * The dock destination the product in front belongs to, or null for a
   * product a link opened. Choosing a destination sends the one in front
   * behind it instead of ending it, so Chat keeps running while Browse is open.
   */
  private where: Destination | null;
  private readonly addresses: DockAddresses;
  /**
   * The products running behind the one in front: a dock destination's under
   * the destination, and an app opened by a link or an address under
   * {@link appKey}, one per product, until it is closed.
   */
  private readonly held = new Map<string, Held>();
  /** When each open product was opened, so the shelf keeps them in that order. */
  private readonly openedOrder = new WeakMap<OpenProduct, number>();
  private openings = 0;

  constructor(private readonly deps: NavigationDeps) {
    // A tab starts in Chat. A product the tab reopens after a reload moves it
    // with {@link restore}.
    this.where = "chat";
    // Only a dev server reads an address set in this browser, so tests can
    // point Browse at a fixture product; a build uses the network's own.
    this.addresses = import.meta.env.DEV
      ? loadAddresses(localStorage, deps.network.id)
      : defaultAddresses(deps.network.id);
  }

  get product(): OpenProduct | null {
    return this.front;
  }

  get opened(): Opened | null {
    return this.how;
  }

  /** Whether the product in front was opened under an entered product id. */
  get entered(): boolean {
    return this.enteredId;
  }

  get overrideOrigin(): string | null {
    return this.origin;
  }

  get sandboxStatus(): string {
    return this.sandbox;
  }

  get askedDomains(): string[] {
    return this.domains;
  }

  get destination(): Destination | null {
    return this.where;
  }

  get dockAddresses(): DockAddresses {
    return this.addresses;
  }

  /** Record domains the core asked about for the product in front. */
  askDomains(domains: string[]): void {
    this.domains = [...new Set([...this.domains, ...domains])];
  }

  /** Whether a product runs behind under `key`. */
  isHeld(key: string): boolean {
    return this.held.has(key);
  }

  /** Show `destination` as restored from the tab, before its product opens. */
  restore(destination: Destination | null): void {
    this.where = destination;
  }

  /** The products open in this tab, in the order they were opened, the one in front marked. */
  openApps(): ShelfApp[] {
    const network = this.deps.network;
    const app = (
      key: string,
      open: OpenProduct,
      how: Opened | null,
      front: boolean,
    ): ShelfApp & { instance: OpenProduct } => ({
      instance: open,
      key,
      // A name reads as itself; a URL product by its id, such as its host.
      name:
        how?.via === "name"
          ? displayAddress(how.address, network)
          : open.productId,
      productId: open.productId,
      front,
    });
    const order = (open: OpenProduct) => this.openedOrder.get(open) ?? 0;
    return [
      ...(this.front === null
        ? []
        : [app("front", this.front, this.how, true)]),
      ...[...this.held].map(([key, entry]) =>
        app(key, entry.product, entry.opened, false),
      ),
    ]
      .sort((left, right) => order(left.instance) - order(right.instance))
      .map(({ instance: _instance, ...shown }) => shown);
  }

  /** Close a product from the Apps shelf: the one in front, or one running behind. */
  closeApp(key: string): void {
    void this.deps.serial(async () => {
      if (key === "front") {
        if (this.front === null) return;
        const closed = this.closedActivity(this.front, this.how);
        this.closeProduct();
        sessionStorage.removeItem(tabProductKey(this.deps.network.id));
        this.deps.activity(closed);
        return;
      }
      const entry = this.held.get(key);
      if (entry === undefined) return;
      entry.product.dispose();
      this.held.delete(key);
      const back = DESTINATIONS.find((known) => known === key) ?? null;
      const closed = this.closedActivity(entry.product, entry.opened);
      this.deps.activity({
        ...closed,
        note: `It ran behind${isProductDestination(back) ? ` ${DESTINATION_LABELS[back]}` : " as an app"}.`,
      });
      this.deps.render();
    });
  }

  /** Close the product in front, as the Product tab's Close does. */
  closeFront(): void {
    void this.deps.serial(async () => {
      const closed =
        this.front === null ? null : this.closedActivity(this.front, this.how);
      this.closeProduct();
      sessionStorage.removeItem(tabProductKey(this.deps.network.id));
      if (closed !== null) this.deps.activity(closed);
    });
  }

  /** The log's event for closing `product`. */
  private closedActivity(product: OpenProduct, how: Opened | null) {
    return {
      category: "products" as const,
      action: "Closed",
      subject: productName(product, how),
      details: [
        { label: "Product id", value: product.productId },
        { label: "Execution", value: product.executionKind },
      ],
    };
  }

  /**
   * Run a session change with the product closed.
   *
   * A product runtime must not outlive the session it was opened under, so
   * the product is closed first and opened again once the new session is in
   * place. Products held behind belong to the old session and end.
   */
  async aroundSessionChange(
    runtime: Runtime,
    change: () => Promise<void>,
  ): Promise<void> {
    const reopen = this.front;
    const how = this.how;
    const entered = this.enteredId;
    const scope = this.origin ?? "";
    this.closeHeld();
    this.closeProduct();
    try {
      await change();
    } finally {
      // A card's Widget is opened again as a Widget, not as the product's App.
      if (reopen && how)
        await this.openTarget(
          runtime,
          { ...how, url: reopen.url },
          reopen.productId,
          entered,
          scope,
          reopen.executionKind === "Widget" ? "Widget" : "App",
        );
    }
  }

  closeProduct(): void {
    this.deps.loading.clear();
    this.domains = [];
    this.front?.dispose();
    this.front = null;
    this.how = null;
    this.sandbox = "";
    this.deps.render();
  }

  /** End every product held behind the one in front. They belong to the session that opened them. */
  private closeHeld(): void {
    for (const entry of this.held.values()) entry.product.dispose();
    this.held.clear();
  }

  /**
   * Where the product in front runs behind: its destination, or its own key as
   * an app. Null when it cannot run behind, as a Pocket card's Widget cannot.
   */
  holdKey(): string | null {
    if (this.front === null) return null;
    if (isProductDestination(this.where)) return this.where;
    return this.front.executionKind === "App"
      ? appKey(this.front.productId)
      : null;
  }

  /**
   * Send the product in front behind the dock, still running, or end it when it
   * cannot run behind. Either way nothing is in front afterwards.
   */
  private leaveFront(): void {
    const key = this.holdKey();
    if (key === null || this.front === null) {
      this.closeProduct();
      return;
    }
    this.deps.loading.clear();
    this.front.setVisible(false);
    this.held.set(key, {
      product: this.front,
      opened: this.how,
      entered: this.enteredId,
      origin: this.origin,
      sandbox: this.sandbox,
      domains: this.domains,
    });
    this.front = null;
    this.how = null;
    this.sandbox = "";
    this.domains = [];
    this.enteredId = false;
    this.origin = null;
    this.deps.render();
  }

  /** Bring a held product back in front exactly as it was left. */
  private resumeFront(back: Held): void {
    this.front = back.product;
    this.how = back.opened;
    this.enteredId = back.entered;
    this.origin = back.origin;
    this.sandbox = back.sandbox;
    this.domains = back.domains;
    this.front.setVisible(true);
    this.deps.showAddress(
      displayAddress(this.how?.address ?? "", this.deps.network),
    );
    this.rememberFront();
    this.deps.render();
    this.deps.resumed();
  }

  /** Keep this tab's way back to the product in front across a reload. */
  private rememberFront(): void {
    if (this.front === null || this.how === null) return;
    sessionStorage.setItem(
      tabProductKey(this.deps.network.id),
      JSON.stringify({
        address: this.how.address,
        productId: this.front.productId,
        destination: this.where,
      }),
    );
  }

  /**
   * Put a dock destination in front: resume it if it is running behind. With
   * `open`, a destination with nothing behind it also opens its address;
   * without it, the destination shows its own screen, such as Browse's
   * landing. The product that was in front keeps running behind its own
   * destination.
   */
  selectDestination(next: Destination, open = false): Promise<void> {
    return this.deps.serial(async (runtime) => {
      if (
        this.where === next &&
        (this.front !== null || !isProductDestination(next) || !open)
      ) {
        // Nothing changes here, but a caller may have changed what Browse shows.
        this.deps.render();
        return;
      }
      if (this.where !== next) this.leaveFront();
      this.where = next;
      if (!isProductDestination(next) || this.deps.testUser) {
        // The address bar names a product, and a host screen is not one.
        this.deps.clearAddress();
        this.deps.render();
        return;
      }
      const back = this.held.get(next);
      if (back !== undefined) {
        this.held.delete(next);
        this.resumeFront(back);
        return;
      }
      this.deps.render();
      const address = this.addresses[next];
      if (!open || address === "") return;
      await this.openSaved(runtime, address);
    });
  }

  /**
   * Open Browse's own address, the Browse app, in front of Browse, when
   * Browse shows with nothing in front. The user chose it, so a test user's
   * tab opens it too.
   */
  openCatalog(): Promise<void> {
    return this.deps.serial(async (runtime) => {
      const address = this.addresses.browse;
      if (this.where !== "browse" || this.front !== null || address === "")
        return;
      await this.openSaved(runtime, address);
    });
  }

  /** Open a saved address as it was opened before: the name or URL, with the id it derives. */
  private async openSaved(runtime: Runtime, address: string): Promise<void> {
    let parsed: Address;
    try {
      parsed = parseTypedAddress(address, this.deps.network);
    } catch (error) {
      this.report(errorText(error), address);
      return;
    }
    const { id } = effectiveProductIdFor(parsed, "");
    if (this.resumeOnLanding(id)) return;
    await this.openResolved(runtime, parsed, id, false);
  }

  /**
   * Send the product in front of a product destination to the Apps shelf,
   * still running, and leave the destination showing nothing in front: Browse
   * shows its landing. Elsewhere Minimize goes back to a host screen.
   */
  minimizeToLanding(): Promise<void> {
    return this.deps.serial(async () => {
      if (!isProductDestination(this.where) || this.front === null) return;
      if (this.front.executionKind !== "App") return;
      const key = appKey(this.front.productId);
      if (this.held.has(key)) return;
      const where = this.where;
      this.where = null;
      this.leaveFront();
      this.where = where;
      this.deps.clearAddress();
      sessionStorage.removeItem(tabProductKey(this.deps.network.id));
      this.deps.render();
    });
  }

  /**
   * On a product destination with nothing in front, bring `productId` back in
   * front when it runs in the Apps shelf, instead of opening it twice.
   */
  private resumeOnLanding(productId: string): boolean {
    if (!isProductDestination(this.where) || this.front !== null) return false;
    const key = appKey(productId);
    const back = this.held.get(key);
    if (back === undefined) return false;
    this.held.delete(key);
    this.resumeFront(back);
    return true;
  }

  /**
   * Say on the status line that `subject` did not open, and log it with the
   * `reason`, which is the status line's text unless given apart.
   */
  private report(
    text: string,
    subject: string,
    reason = text,
    details?: ActivityDetail[],
  ): void {
    this.deps.statusLine.textContent = text;
    this.deps.activity({
      category: "products",
      action: "Not opened",
      subject,
      note: reason,
      failed: true,
      ...(details === undefined ? {} : { details }),
    });
  }

  /**
   * A product asked the host to open `url`. A name of this network opens in
   * front of the destination the request came from, which keeps running behind
   * it. A destination's own product is resumed instead of opened twice.
   */
  navigateFromProduct(url: string): Promise<void> {
    const network = this.deps.network;
    const where = resolveNavigation(url, network);
    if (where.kind === "external") {
      this.deps.activity({
        category: "products",
        action: "Navigate requested",
        subject: where.url,
        note: "Opened in a new browser tab.",
      });
      window.open(where.url, "_blank", "noopener,noreferrer");
      return Promise.resolve();
    }
    if (where.kind === "refused") {
      this.deps.activity({
        category: "products",
        action: "Navigate refused",
        subject: url,
        note: where.reason,
        failed: true,
      });
      return Promise.reject(new Error(where.reason));
    }
    this.deps.activity({
      category: "products",
      action: "Navigate requested",
      subject: where.address,
      ...(url === where.address
        ? {}
        : { details: [{ label: "Asked for", value: url }] }),
    });
    // The host has taken the request once it is queued. What it opens is
    // reported in the menu and the log, not back to the product.
    const known = destinationOf(where.address, this.addresses, network);
    if (known !== null) void this.selectDestination(known, true);
    else
      void this.deps.serial(async (runtime) => {
        let parsed: Address;
        try {
          parsed = parseTypedAddress(where.address, network);
        } catch (error) {
          this.report(errorText(error), where.address);
          return;
        }
        const { id } = effectiveProductIdFor(parsed, "");
        // An app already open comes to the front as it was, as on iOS.
        if (this.where === null && this.holdKey() === appKey(id)) return;
        if (this.bringApp(appKey(id))) return;
        this.leaveFront();
        this.where = null;
        this.deps.render();
        await this.openResolved(runtime, parsed, id, false);
      });
    return Promise.resolve();
  }

  /**
   * Bring the app held under `key` to the front, sending the one in front
   * behind. False when no app is held there. Runs inside
   * {@link NavigationDeps.serial}.
   */
  bringApp(key: string): boolean {
    const back = this.held.get(key);
    if (back === undefined) return false;
    this.held.delete(key);
    // Over Browse's landing the app comes back in Browse, where it was minimized.
    if (!isProductDestination(this.where) || this.front !== null) {
      this.leaveFront();
      this.where = null;
    }
    this.resumeFront(back);
    return true;
  }

  /**
   * Open what was typed in the address bar. Over a host screen or an app it
   * opens as an app of its own, and one already running behind comes to the
   * front instead, which returns true. Runs inside
   * {@link NavigationDeps.serial}.
   */
  async openTyped(
    runtime: Runtime,
    address: Address,
    productId: string,
    entered: boolean,
  ): Promise<boolean> {
    if (!isProductDestination(this.where)) {
      if (this.holdKey() !== appKey(productId)) {
        if (this.bringApp(appKey(productId))) return true;
        this.leaveFront();
      }
      this.where = null;
    } else if (this.resumeOnLanding(productId)) return true;
    await this.openResolved(runtime, address, productId, entered);
    return false;
  }

  /**
   * Turn what the bar holds into a URL to embed.
   *
   * A URL is used as typed. A name is looked up on Asset Hub, and its content is
   * loaded by the sandbox loader, a static page of this host, which mounts it
   * under this host's own path for the active wallet and this product.
   */
  private async resolveTarget(
    address: Address,
    productId: string,
  ): Promise<Target> {
    const network = this.deps.network;
    if (address.kind === "url")
      return { address: address.url.href, via: "url", url: address.url };
    const shown = address.suffix
      ? `${address.name}/${address.suffix}`
      : address.name;
    requireSecureHost(window.isSecureContext, window.location.origin);
    this.deps.statusLine.textContent = `Resolving ${address.name} on ${network.displayName} Asset Hub…`;
    let choice: Awaited<ReturnType<typeof resolveSource>>;
    try {
      choice = await resolveSource(address.name, network);
    } catch (error) {
      throw new Error(
        `Could not read ${address.name}: ${errorText(error)}. Check the connection, or enter a URL.`,
      );
    }
    const { source, skipped } = choice;
    if (source === null)
      throw new Error(notOpenableText(address.name, skipped));
    const scope = mountScope(this.deps.base, {
      walletId: this.deps.walletId() ?? "signed-out",
      productId,
      cid: source.cid,
      network: network.id,
    });
    return {
      address: shown,
      via: "name",
      cid: source.cid,
      kind: source.kind,
      mount: scope,
      record: source.record,
      skipped,
      url: loaderUrl({
        base: this.deps.base,
        origin: window.location.origin,
        scope,
        gateway: network.dotns.contentGateway,
        kind: source.kind,
        start: `/${address.suffix}`,
      }),
    };
  }

  /**
   * Embed an already resolved target, closing the current product first. The
   * bookkeeping for the bar and the entered id changes only once the product is
   * open.
   */
  async openTarget(
    runtime: Runtime,
    target: Target,
    productId: string,
    entered: boolean,
    scope: string,
    executionKind: "App" | "Widget" = "App",
  ): Promise<void> {
    const network = this.deps.network;
    this.closeProduct();
    const op = this.deps.loading.begin("Loading…");
    this.sandbox = "";
    // A product sent behind the dock while it loads must not write to the screen.
    let handle: OpenProduct | null = null;
    const inFront = (): boolean => handle === null || handle === this.front;
    try {
      handle = this.front = await openProduct(
        runtime,
        target.url,
        productId,
        this.deps.slot,
        {
          mounted: target.via === "name",
          executionKind,
          tap: this.deps.tap(runtime, productId),
          onFrameLoad: (count) => {
            if (count < documentsBeforeProduct(target.via)) return;
            this.deps.loading.finish(op);
            if (!inFront()) return;
            this.sandbox = "";
            this.deps.render();
          },
          onSandboxStatus: (state, detail) => {
            if (state === "error") {
              this.deps.loading.finish(op);
              this.deps.activity({
                category: "products",
                action: "Sandbox failed",
                subject: productId,
                note: detail,
                failed: true,
              });
            }
            if (!inFront()) return;
            this.sandbox = `${state === "error" ? SANDBOX_FAILED : "Sandbox:"} ${detail}`;
            this.deps.render();
          },
        },
      );
      this.openings += 1;
      this.openedOrder.set(handle, this.openings);
      this.how = {
        address: target.address,
        via: target.via,
        cid: target.cid,
        kind: target.kind,
        mount: target.mount,
        record: target.record,
        skipped: target.skipped,
      };
      this.enteredId = entered;
      this.domains = [];
      this.origin = entered ? scope : null;
      this.deps.showAddress(displayAddress(target.address, network));
      this.deps.render();
      // A Widget opens only from its card: the address, typed or reloaded,
      // would open the product's App instead.
      if (executionKind === "App") {
        this.deps.recorded({ address: target.address, productId, entered });
        this.rememberFront();
      }
      this.deps.render();
      this.deps.activity({
        category: "products",
        action: "Opened",
        subject: target.via === "name" ? target.address : productId,
        details: [
          {
            label: "Product id",
            value: entered ? `${productId} (entered, not verified)` : productId,
          },
          { label: "Execution", value: executionKind },
          ...(target.via === "name"
            ? [
                { label: "Record", value: target.record ?? "" },
                { label: "Content", value: target.cid ?? "" },
                { label: "Loader", value: target.url.href },
              ]
            : [{ label: "URL", value: target.url.href }]),
        ].filter(({ value }) => value !== ""),
      });
    } catch (error) {
      this.deps.loading.finish(op);
      this.report(
        `Could not open ${target.url.href} as ${productId}: ${errorText(error)}`,
        target.via === "name" ? target.address : productId,
        errorText(error),
        [
          { label: "Product id", value: productId },
          { label: "Execution", value: executionKind },
          { label: "URL", value: target.url.href },
        ],
      );
    }
  }

  /**
   * Open a Pocket card: its product's Widget, at the card named in the query,
   * in front of Pocket. The Widget runs as the product, so it shares the
   * worker's and the app's storage, accounts and grants. Which content runs is
   * `findWidget`'s rule; Pocket stays in front, and says why on `status`, when
   * there is none to run. Choosing Pocket in the dock ends the Widget.
   */
  openPocketCard(key: PocketCardKey, status: HTMLElement): Promise<void> {
    const network = this.deps.network;
    return this.deps.serial(async (runtime) => {
      status.textContent = `Looking up ${key.productId}'s widget…`;
      let widget: Awaited<ReturnType<typeof resolveWidget>>;
      try {
        requireSecureHost(window.isSecureContext, window.location.origin);
        widget = await resolveWidget(key.productId, network);
      } catch (error) {
        status.textContent = errorText(error);
        this.deps.activity({
          category: "products",
          action: "Widget not opened",
          subject: key.productId,
          note: errorText(error),
          failed: true,
          details: [{ label: "Card", value: key.cardId }],
        });
        return;
      }
      if (this.where !== "pocket") {
        status.textContent = "";
        return;
      }
      status.textContent = "";
      if (widget.fromBaseName)
        this.deps.activity({
          category: "products",
          action: "Widget from base name",
          subject: key.productId,
          note: `${key.productId} publishes no widget; its base name's content runs as the Widget, as the iOS host serves it.`,
        });
      this.leaveFront();
      this.where = null;
      sessionStorage.removeItem(tabProductKey(network.id));
      const start = cardLaunchPath(key.cardId);
      const scope = mountScope(this.deps.base, {
        walletId: this.deps.walletId() ?? "signed-out",
        productId: key.productId,
        cid: widget.cid,
        network: network.id,
      });
      await this.openTarget(
        runtime,
        {
          address: `${key.productId}${start}`,
          via: "name",
          cid: widget.cid,
          kind: widget.kind,
          mount: scope,
          record: widget.record,
          skipped: [],
          url: loaderUrl({
            base: this.deps.base,
            origin: window.location.origin,
            scope,
            gateway: network.dotns.contentGateway,
            kind: widget.kind,
            start,
          }),
        },
        key.productId,
        false,
        key.productId,
        "Widget",
      );
    });
  }

  /**
   * Resolve `address` and open it as `productId`. The current product stays
   * until resolution succeeds, so a lookup that fails changes nothing.
   */
  async openResolved(
    runtime: Runtime,
    address: Address,
    productId: string,
    entered: boolean,
  ): Promise<void> {
    let target: Target;
    const op = this.deps.loading.begin(
      address.kind === "name" ? "Looking up name…" : "Loading…",
    );
    try {
      target = await this.resolveTarget(address, productId);
    } catch (error) {
      this.deps.loading.finish(op);
      this.report(
        errorText(error),
        address.kind === "name" ? address.name : address.url.href,
      );
      return;
    }
    await this.openTarget(
      runtime,
      target,
      productId,
      entered,
      scopeOf(address),
    );
  }

  /** The product this tab opened itself before a reload, or null. */
  readTabProduct(): TabProduct | null {
    const network = this.deps.network;
    try {
      const stored: unknown = JSON.parse(
        sessionStorage.getItem(tabProductKey(network.id)) ?? "null",
      );
      if (typeof stored !== "object" || stored === null) return null;
      const { address, productId, destination } = stored as Record<
        string,
        unknown
      >;
      if (typeof address !== "string" || typeof productId !== "string")
        return null;
      return {
        address,
        productId,
        destination:
          DESTINATIONS.find((known) => known === destination) ?? null,
        parsed: parseTypedAddress(address, network),
      };
    } catch {
      return null;
    }
  }
}
