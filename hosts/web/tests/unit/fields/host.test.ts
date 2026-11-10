import { afterEach, describe, expect, test } from "bun:test";
import type { PromptRequest } from "../../../src/app/prompt.js";
import { FieldHost, type ShippingLabel } from "../../../src/fields/host.js";
import {
  readMessage,
  type Listing,
  type Purchase,
  type Received,
} from "../../../bridges/fields/protocol.js";
import type { FieldTransport } from "../../../src/fields/transport.js";
import {
  cryptoRandom,
  fromHex,
  toHex,
  utf8,
} from "../../../bridges/primitives/bytes.js";
import { digest, sealTo } from "../../../src/invitations/sealing.js";
import { FIELD_DEMO } from "../../../bridges/fields/protocol.js";

const ADDRESS = "Ada Example\n24 Sample Lane\nExample City";
const ADDRESS_LINES = ["Ada Example", "24 Sample Lane", "Example City"];
const SECOND_ADDRESS = "80 Fixture Road, Test Town";
const PHONE = "+1 555 010 0123";
const account = (byte: string) => `0x${byte.repeat(64)}`;
const HOUR = 3_600_000;
const hosts: FieldHost[] = [];
const ports: MessagePort[] = [];
afterEach(() => {
  for (const host of hosts.splice(0)) host.setScope(null);
  for (const port of ports.splice(0)) port.close();
});

type Snapshot = {
  listings: Listing[];
  orders: { id: string; listing: string; item: string; buyer: string }[];
  purchases: { id: string; listing: string; item: string; seller: string }[];
};

function setup() {
  const messages: Received[] = [];
  const listeners: ((message: Received) => void)[] = [];
  let clock = 1_800_000_000_000;
  /** Deliver `received` to every open connection; its statement lives an hour unless said otherwise. */
  const emit = (
    received: Omit<Received, "expires"> & { expires?: number },
  ): number => {
    const full = { ...received, expires: received.expires ?? clock + HOUR };
    messages.push(full);
    for (const listener of listeners) listener(full);
    return full.expires;
  };
  const makeHost = (sender: string) => {
    const sheets: PromptRequest<unknown>[] = [];
    const labels: ShippingLabel[] = [];
    let printing: ((signal: AbortSignal) => Promise<void>) | null = null;
    let choice: (sheet: PromptRequest<unknown>) => unknown = (sheet) =>
      sheet.choices[0]!.value;
    const host = new FieldHost({
      ask: async <T>(sheet: PromptRequest<T>) => {
        sheets.push(sheet as PromptRequest<unknown>);
        return choice(sheet as PromptRequest<unknown>) as T;
      },
      open: async (_product, receive): Promise<FieldTransport> => {
        listeners.push(receive);
        for (const message of messages) receive(message);
        return {
          account: sender,
          network: { kind: "simulated", name: "test" },
          check: () => {},
          post: async (message) => emit({ sender, message }),
          close: () => {
            const index = listeners.indexOf(receive);
            if (index >= 0) listeners.splice(index, 1);
          },
        };
      },
      viewUrl: (token, theme) => `view:${token}?${theme}`,
      print: async (label, signal) => {
        labels.push(label);
        await printing?.(signal);
      },
      now: () => clock,
    });
    hosts.push(host);
    host.setScope(sender);
    const connect = (product: string, role: "provider" | "market") => {
      const frame = { product };
      const channel = new MessageChannel();
      ports.push(channel.port1);
      host.connect(product, role, channel.port2, () => frame);
      const received: unknown[] = [];
      let nextId = 1;
      const pending = new Map<
        number,
        { resolve(value: unknown): void; reject(error: Error): void }
      >();
      channel.port1.onmessage = (event: MessageEvent) => {
        received.push(event.data);
        const data = event.data as {
          id?: number;
          result?: unknown;
          error?: { code: string; message: string };
        };
        if (data.id === undefined) return;
        const waiting = pending.get(data.id);
        pending.delete(data.id);
        if (data.error !== undefined)
          waiting?.reject(
            Object.assign(new Error(data.error.message), {
              code: data.error.code,
            }),
          );
        else waiting?.resolve(data.result);
      };
      channel.port1.start();
      const call = (request: Record<string, unknown>): Promise<unknown> =>
        new Promise((resolve, reject) => {
          const id = nextId++;
          pending.set(id, { resolve, reject });
          channel.port1.postMessage({ ...request, id });
        });
      /** A host-origin frame nested directly in this product. */
      const child = { parent: frame };
      return { call, received, child };
    };
    return {
      host,
      connect,
      sheets,
      labels,
      choose: (next: typeof choice) => {
        choice = next;
      },
      holdPrint: (hold: (signal: AbortSignal) => Promise<void>) => {
        printing = hold;
      },
    };
  };
  return {
    messages,
    emit,
    makeHost,
    tick: (ms: number) => {
      clock += ms;
    },
  };
}

/** Redeem `view` for `source`, and every later message its port gets. */
function redeem(host: FieldHost, view: string, source: unknown) {
  const token = view.slice("view:".length).split("?")[0];
  const channel = new MessageChannel();
  ports.push(channel.port1);
  const heard: unknown[] = [];
  let wake = () => {};
  channel.port1.onmessage = (event) => {
    heard.push(event.data);
    wake();
  };
  channel.port1.start();
  const next = (count: number) =>
    new Promise<unknown>((resolve) => {
      const check = () => {
        if (heard.length >= count) resolve(heard[count - 1]);
      };
      wake = check;
      check();
    });
  host.attachView(token, source, channel.port2);
  return { first: next(1), next };
}

async function register(
  call: (request: Record<string, unknown>) => Promise<unknown>,
  address = ADDRESS,
) {
  await call({ op: "declare", name: "Notebook", fields: ["address", "phone"] });
  await call({ op: "save", values: { address, phone: PHONE } });
}

/** Bob lists, Alice buys from her one provider. */
async function sold() {
  const world = setup();
  const alice = world.makeHost(account("1"));
  const bob = world.makeHost(account("2"));
  await register(alice.connect("provider.test", "provider").call);
  const seller = bob.connect("market.test", "market");
  await seller.call({ op: "list", item: "Blue notebook" });
  const buyer = alice.connect("market.test", "market");
  const board = (await buyer.call({ op: "refresh" })) as Snapshot;
  const { order } = (await buyer.call({
    op: "buy",
    listing: board.listings[0]!.id,
  })) as { order: string };
  /** The same order as the seller's host lists it. */
  const sale = ((await seller.call({ op: "refresh" })) as Snapshot).orders[0]!
    .id;
  return { ...world, alice, bob, seller, buyer, order, sale };
}

describe("web-only field-provider boundary", () => {
  test("selects one declared provider and seals only the address to the seller's host", async () => {
    const { makeHost, messages } = setup();
    const alice = makeHost(account("1"));
    const bob = makeHost(account("2"));
    const provider = alice.connect("provider-one.test", "provider");
    const second = alice.connect("provider-two.test", "provider");
    const buyer = alice.connect("market.test", "market");
    const seller = bob.connect("market.test", "market");
    await register(provider.call);
    await register(second.call, SECOND_ADDRESS);
    await seller.call({ op: "list", item: "Blue notebook" });
    const snapshot = (await buyer.call({ op: "refresh" })) as Snapshot;
    alice.choose((sheet) =>
      sheet.title === "Share your address with the seller?"
        ? "provider-two.test"
        : sheet.choices[0]!.value,
    );
    const { order } = (await buyer.call({
      op: "buy",
      listing: snapshot.listings[0]!.id,
    })) as { order: string };
    const delivered = (await seller.call({ op: "refresh" })) as Snapshot;
    expect(delivered.orders).toEqual([
      {
        // The seller names an order by its id and its buyer, so another
        // signer's copy of the id is another order, never a replacement.
        id: `${order}:${account("1")}`,
        listing: snapshot.listings[0]!.id,
        item: "Blue notebook",
        buyer: account("1"),
        at: expect.any(Number),
      } as never,
    ]);
    const { view } = (await seller.call({
      op: "address",
      order: delivered.orders[0]!.id,
      theme: "paper",
    })) as { view: string };
    expect(await redeem(bob.host, view, seller.child).first).toEqual({
      lines: [SECOND_ADDRESS],
    });
    const publicData = JSON.stringify([
      messages,
      buyer.received,
      seller.received,
      provider.received,
      second.received,
    ]);
    for (const secret of [ADDRESS, ...ADDRESS_LINES, SECOND_ADDRESS, PHONE])
      expect(publicData).not.toContain(secret);
    // Two providers: still one sheet, each choice naming what it shares.
    expect(alice.sheets.map((sheet) => sheet.title)).toEqual([
      "Share your address with the seller?",
    ]);
    const sheet = alice.sheets[0]!;
    expect(sheet.choices.map((item) => [item.label, item.value])).toEqual([
      [
        `Keys match, share ${ADDRESS_LINES.join(", ")} · from Notebook · provider-one.test`,
        "provider-one.test",
      ],
      [
        `Keys match, share ${SECOND_ADDRESS} · from Notebook · provider-two.test`,
        "provider-two.test",
      ],
      ["Cancel", null],
    ]);
    expect(
      sheet.fields.find((field) => field.label === "Seller's delivery key")
        ?.value,
    ).toBe(snapshot.listings[0]!.key);
  });

  test("a view token opens once, only in a frame nested directly in the product it was issued to", async () => {
    const { bob, seller, buyer, alice, sale } = await sold();
    const issue = async () =>
      ((await seller.call({ op: "address", order: sale })) as { view: string })
        .view;
    // The product window itself, a frame nested one level deeper, another
    // product's frame and a missing source are all refused.
    for (const source of [
      { parent: {} },
      { parent: { parent: seller.child } },
      buyer.child,
      null,
    ])
      expect(await redeem(bob.host, await issue(), source).first).toEqual({
        unavailable: true,
      });
    const view = await issue();
    expect(await redeem(bob.host, view, seller.child).first).toEqual({
      lines: ADDRESS_LINES,
    });
    expect(await redeem(bob.host, view, seller.child).first).toEqual({
      unavailable: true,
    });
    // A token issued in one host session is unknown to another host.
    expect(await redeem(alice.host, await issue(), seller.child).first).toEqual(
      { unavailable: true },
    );
  });

  test("an unused token expires, and ending the session blanks views already drawn", async () => {
    const { bob, seller, sale, tick } = await sold();
    const stale = (
      (await seller.call({ op: "address", order: sale })) as {
        view: string;
      }
    ).view;
    tick(61_000);
    expect(await redeem(bob.host, stale, seller.child).first).toEqual({
      unavailable: true,
    });
    const live = (
      (await seller.call({ op: "address", order: sale })) as {
        view: string;
      }
    ).view;
    const drawn = redeem(bob.host, live, seller.child);
    expect(await drawn.first).toEqual({ lines: ADDRESS_LINES });
    bob.host.setScope(null);
    expect(await drawn.next(2)).toEqual({ clear: true });
  });

  test("the buyer's host draws the buyer's own order; nobody else's order opens", async () => {
    const { alice, buyer, seller, emit, messages, order, makeHost } =
      await sold();
    const mine = (
      (await buyer.call({ op: "address", order })) as {
        view: string;
      }
    ).view;
    expect(await redeem(alice.host, mine, buyer.child).first).toEqual({
      lines: ADDRESS_LINES,
    });
    const outsider = makeHost(account("3")).connect("market.test", "market");
    await expect(outsider.call({ op: "address", order })).rejects.toThrow(
      "No such order",
    );
    await expect(buyer.call({ op: "print", order })).rejects.toThrow(
      "No such order",
    );
    // A copied box under another id and buyer does not open, so the seller
    // never keeps it.
    const purchase = messages.at(-1)!.message as Purchase;
    const forged = crypto.randomUUID();
    emit({ sender: account("3"), message: { ...purchase, id: forged } });
    expect(
      ((await seller.call({ op: "refresh" })) as Snapshot).orders.map(
        (sale) => sale.buyer,
      ),
    ).toEqual([account("1")]);
    await expect(
      seller.call({ op: "address", order: `${forged}:${account("3")}` }),
    ).rejects.toThrow("No such order");
  });

  test("prints a host-composed label for the seller, and the reply carries no address", async () => {
    const { bob, seller, order, sale } = await sold();
    expect(await seller.call({ op: "print", order: sale })).toEqual({
      printed: true,
    });
    expect(bob.labels).toEqual([
      {
        order,
        item: "Blue notebook",
        product: "market.test",
        address: ADDRESS_LINES,
        at: expect.any(Number),
      } as never,
    ]);
    expect(JSON.stringify(seller.received)).not.toContain("Sample Lane");
  });

  test("requires declarations, rejects undeclared fields and keeps provider operations out of the marketplace", async () => {
    const { makeHost } = setup();
    const alice = makeHost(account("1"));
    const provider = alice.connect("provider.test", "provider");
    await expect(
      provider.call({ op: "save", values: { address: ADDRESS } }),
    ).rejects.toThrow("Register declarations first");
    await provider.call({ op: "declare", name: "Notebook", fields: ["phone"] });
    await expect(
      provider.call({ op: "save", values: { address: ADDRESS } }),
    ).rejects.toThrow("declared fields");
    await expect(
      provider.call({ op: "declare", name: "Notebook", fields: ["password"] }),
    ).rejects.toThrow("Declare address");
    const buyer = alice.connect("market.test", "market");
    await expect(
      buyer.call({ op: "save", values: { address: ADDRESS } }),
    ).rejects.toThrow("not supported");
    await expect(
      provider.call({ op: "address", order: "private" }),
    ).rejects.toThrow("not supported");
  });

  test("missing provider and denied consent publish nothing, and a reset removes declarations", async () => {
    const { makeHost, messages } = setup();
    const alice = makeHost(account("1"));
    const bob = makeHost(account("2"));
    const seller = bob.connect("market.test", "market");
    await seller.call({ op: "list", item: "Blue notebook" });
    const buyer = alice.connect("market.test", "market");
    const snapshot = (await buyer.call({ op: "refresh" })) as Snapshot;
    const listing = snapshot.listings[0]!.id;
    await expect(buyer.call({ op: "buy", listing })).rejects.toThrow(
      "No address saved",
    );
    const provider = alice.connect("provider.test", "provider");
    await register(provider.call);
    alice.choose((sheet) =>
      sheet.title === "Share your address with the seller?"
        ? null
        : sheet.choices[0]!.value,
    );
    await expect(buyer.call({ op: "buy", listing })).rejects.toThrow(
      "cancelled",
    );
    expect(messages).toHaveLength(1);
    alice.host.setScope(null);
    alice.host.setScope("another-session");
    const fresh = alice.connect("market.test", "market");
    await fresh.call({ op: "refresh" });
    await expect(fresh.call({ op: "buy", listing })).rejects.toThrow(
      "No address saved",
    );
  });

  test("a new session cannot open an order the old session's key could", async () => {
    const { bob, sale } = await sold();
    bob.host.setScope(null);
    bob.host.setScope("new-session");
    const fresh = bob.connect("market.test", "market");
    await expect(fresh.call({ op: "address", order: sale })).rejects.toThrow(
      "No such order",
    );
  });

  test("public parser rejects malformed envelopes and drops extra fields", () => {
    const listing: Listing = {
      v: 1,
      type: "listing",
      id: crypto.randomUUID(),
      item: "Test item",
      seller: account("1"),
      key: account("2"),
    };
    expect(readMessage({ ...listing, address: ADDRESS })).toEqual(listing);
    expect(readMessage({ ...listing, key: "not a key" })).toBeNull();
    expect(readMessage({ ...listing, item: "a".repeat(81) })).toBeNull();
    expect(
      readMessage({
        v: 1,
        type: "purchase",
        id: listing.id,
        listing: listing.id,
        box: "0x01",
      }),
    ).toBeNull();
  });

  test("rejects item substitution and cross-signer purchase replacement under an existing id", async () => {
    const { bob, seller, buyer, messages, emit, sale } = await sold();
    const listing = ((await buyer.call({ op: "refresh" })) as Snapshot)
      .listings[0]!;
    const purchase = messages.at(-1)!.message as Purchase;
    emit({
      sender: account("2"),
      message: { ...listing, item: "Substituted item" },
    });
    emit({
      sender: account("3"),
      message: { ...purchase, box: "0x" + "01".repeat(100) },
    });
    const { view } = (await seller.call({ op: "address", order: sale })) as {
      view: string;
    };
    expect(await redeem(bob.host, view, seller.child).first).toEqual({
      lines: ADDRESS_LINES,
    });
    expect(
      ((await seller.call({ op: "refresh" })) as Snapshot).orders[0]?.item,
    ).toBe("Blue notebook");
  });

  test("expired unused tokens free their slots, so abandoned views do not exhaust a connection", async () => {
    const { seller, sale, tick } = await sold();
    for (let index = 0; index < 50; index++)
      await seller.call({ op: "address", order: sale });
    await expect(seller.call({ op: "address", order: sale })).rejects.toThrow(
      "Too many open views",
    );
    tick(61_000);
    await expect(seller.call({ op: "address", order: sale })).resolves.toEqual({
      view: expect.stringMatching(/^view:/),
    });
  });

  test("a released view is dropped, so ending the session no longer messages it", async () => {
    const { bob, seller, sale } = await sold();
    const { view } = (await seller.call({ op: "address", order: sale })) as {
      view: string;
    };
    const token = view.slice("view:".length).split("?")[0];
    const channel = new MessageChannel();
    ports.push(channel.port1);
    const heard: unknown[] = [];
    channel.port1.onmessage = (event) => heard.push(event.data);
    channel.port1.start();
    bob.host.attachView(token, seller.child, channel.port2);
    await Bun.sleep(5);
    channel.port1.postMessage({ release: true });
    await Bun.sleep(5);
    bob.host.setScope(null);
    await Bun.sleep(5);
    expect(heard).toEqual([{ lines: ADDRESS_LINES }]);
  });

  test("ending the session aborts a label still printing", async () => {
    const { bob, seller, sale } = await sold();
    let aborted = false;
    bob.holdPrint(
      (signal) =>
        new Promise<void>((_resolve, reject) =>
          signal.addEventListener("abort", () => {
            aborted = true;
            reject(new Error("Session ended."));
          }),
        ),
    );
    const printing = seller.call({ op: "print", order: sale });
    await Bun.sleep(5);
    bob.host.setScope(null);
    await Bun.sleep(5);
    expect(aborted).toBe(true);
    expect(
      await Promise.race([printing, Bun.sleep(20).then(() => "silent")]),
    ).toBe("silent");
  });

  test("refuses an address with more lines than a label shows, instead of cutting it", async () => {
    const { makeHost } = setup();
    const provider = makeHost(account("1")).connect(
      "provider.test",
      "provider",
    );
    await provider.call({
      op: "declare",
      name: "Notebook",
      fields: ["address"],
    });
    await expect(
      provider.call({
        op: "save",
        values: { address: ["1", "2", "3", "4", "5", "6", "7"].join("\n") },
      }),
    ).rejects.toThrow("at most 6 address lines");
    await expect(
      provider.call({
        op: "save",
        values: { address: ["1", "2", "3", "4", "5", "6"].join("\n") },
      }),
    ).resolves.toEqual({ saved: ["address"] });
  });

  test("failures carry a code: a dismissed sheet is cancelled, a bad request invalid", async () => {
    const { makeHost } = setup();
    const bob = makeHost(account("2"));
    const seller = bob.connect("market.test", "market");
    bob.choose(() => false);
    await expect(
      seller.call({ op: "list", item: "Blue notebook" }),
    ).rejects.toMatchObject({ code: "cancelled" });
    const provider = bob.connect("provider.test", "provider");
    await expect(
      provider.call({ op: "declare", fields: ["address"] }),
    ).rejects.toMatchObject({
      code: "invalid",
      message: expect.stringContaining("Name the app"),
    });
    await expect(provider.call({ op: "frobnicate" })).rejects.toMatchObject({
      code: "unsupported",
    });
  });

  test("an unexpected failure crosses as a generic message, not its own text", async () => {
    const world = setup();
    const alice = world.makeHost(account("1"));
    const host = new FieldHost({
      ask: async () => true as never,
      open: async () => {
        throw new Error("internal detail 24 Sample Lane");
      },
      viewUrl: () => "",
      print: async () => {},
    });
    hosts.push(host);
    host.setScope("x");
    const channel = new MessageChannel();
    ports.push(channel.port1);
    host.connect("market.test", "market", channel.port2);
    const reply = new Promise((resolve) => {
      channel.port1.onmessage = (event) => {
        if ((event.data as { id?: number }).id === 1) resolve(event.data);
      };
    });
    channel.port1.start();
    channel.port1.postMessage({ op: "refresh", id: 1 });
    expect(await reply).toEqual({
      id: 1,
      error: { code: "unavailable", message: "Request failed." },
    });
    void alice;
  });

  test("a provider reads back which fields it saved, never their values, after reconnecting", async () => {
    const { makeHost } = setup();
    const alice = makeHost(account("1"));
    await register(alice.connect("provider.test", "provider").call);
    const again = alice.connect("provider.test", "provider");
    const status = await again.call({ op: "status" });
    expect(status).toEqual({
      registered: true,
      name: "Notebook",
      fields: ["address", "phone"],
      saved: ["address", "phone"],
    });
    expect(JSON.stringify(again.received)).not.toContain("Sample Lane");
    expect(
      await alice.connect("other.test", "provider").call({ op: "status" }),
    ).toEqual({ registered: false, name: null, fields: [], saved: [] });
  });

  test("a purchase is one sheet: item, the exact address, its app and verified id, and the key", async () => {
    const { alice, buyer, order } = await sold();
    expect(alice.sheets.map((sheet) => [sheet.title, sheet.from])).toEqual([
      ["Share your address with the seller?", "market.test"],
    ]);
    const fields = Object.fromEntries(
      alice.sheets[0]!.fields.map((field) => [field.label, field.value]),
    );
    expect(fields).toMatchObject({
      Item: "Blue notebook",
      Address: ADDRESS_LINES.join(", "),
      From: "Notebook · provider.test",
    });
    expect(alice.sheets[0]!.choices.map((choice) => choice.value)).toEqual([
      "provider.test",
      null,
    ]);
    // Viewing the approved order, again and again, asks nothing more.
    await buyer.call({ op: "address", order });
    await buyer.call({ op: "address", order });
    expect(alice.sheets).toHaveLength(1);
  });

  test("registering and saving ask nothing; only an app the host made a provider can register", async () => {
    const { makeHost } = setup();
    const alice = makeHost(account("1"));
    await register(alice.connect("provider.test", "provider").call);
    expect(alice.sheets).toEqual([]);
    const market = alice.connect("market.test", "market");
    await expect(
      market.call({ op: "declare", name: "Sneaky", fields: ["address"] }),
    ).rejects.toMatchObject({ code: "unsupported" });
    await expect(
      market.call({ op: "save", values: { address: ADDRESS } }),
    ).rejects.toMatchObject({ code: "unsupported" });
  });

  test("seals the address the sheet showed, even if the provider saves a new one meanwhile", async () => {
    const world = setup();
    const alice = world.makeHost(account("1"));
    const bob = world.makeHost(account("2"));
    const provider = alice.connect("provider.test", "provider");
    await register(provider.call);
    const seller = bob.connect("market.test", "market");
    await seller.call({ op: "list", item: "Blue notebook" });
    const buyer = alice.connect("market.test", "market");
    const board = (await buyer.call({ op: "refresh" })) as Snapshot;
    let changed: Promise<unknown> = Promise.resolve();
    alice.choose((sheet) => {
      changed = provider.call({
        op: "save",
        values: { address: SECOND_ADDRESS, phone: PHONE },
      });
      // Answer only after the new value is saved.
      return changed.then(() => sheet.choices[0]!.value);
    });
    await buyer.call({
      op: "buy",
      listing: board.listings[0]!.id,
    });
    const sale = ((await seller.call({ op: "refresh" })) as Snapshot).orders[0]!
      .id;
    const { view } = (await seller.call({ op: "address", order: sale })) as {
      view: string;
    };
    expect(await redeem(bob.host, view, seller.child).first).toEqual({
      lines: ADDRESS_LINES,
    });
  });

  test("printing asks nothing in the host; the browser's own dialog is the step", async () => {
    const { bob, seller, sale } = await sold();
    const before = bob.sheets.length;
    await seller.call({ op: "print", order: sale });
    await seller.call({ op: "print", order: sale });
    expect(bob.sheets).toHaveLength(before);
    expect(bob.labels).toHaveLength(2);
  });

  test("an expired listing leaves the shop and cannot be bought, even from a sheet left open", async () => {
    const { makeHost, messages, tick } = setup();
    const alice = makeHost(account("1"));
    const bob = makeHost(account("2"));
    await register(alice.connect("provider.test", "provider").call);
    const seller = bob.connect("market.test", "market");
    const buyer = alice.connect("market.test", "market");
    await seller.call({ op: "list", item: "Blue notebook" });
    const first = ((await buyer.call({ op: "refresh" })) as Snapshot)
      .listings[0]!.id;
    tick(HOUR);
    expect(
      ((await buyer.call({ op: "refresh" })) as Snapshot).listings,
    ).toEqual([]);
    await expect(buyer.call({ op: "buy", listing: first })).rejects.toThrow(
      "This listing expired",
    );
    await seller.call({ op: "list", item: "Red notebook" });
    const second = ((await buyer.call({ op: "refresh" })) as Snapshot)
      .listings[0]!.id;
    const posted = messages.length;
    alice.choose((sheet) => {
      tick(HOUR);
      return sheet.choices[0]!.value;
    });
    await expect(buyer.call({ op: "buy", listing: second })).rejects.toThrow(
      "This listing expired",
    );
    expect(messages.length).toBe(posted);
  });

  test("a full board drops what expired, then what came first, never what is new or the seller's own", async () => {
    const { makeHost, emit, tick } = setup();
    const bob = makeHost(account("2"));
    const seller = bob.connect("market.test", "market");
    await seller.call({ op: "list", item: "Own lamp" });
    const own = ((await seller.call({ op: "refresh" })) as Snapshot)
      .listings[0]!.id;
    const listingFrom = (index: number) => {
      const sender = `0x${index.toString(16).padStart(64, "a")}`;
      return {
        v: 1 as const,
        type: "listing" as const,
        id: crypto.randomUUID(),
        item: `Item ${index}`,
        seller: sender,
        key: account("9"),
      };
    };
    const short = listingFrom(0);
    emit({ sender: short.seller, message: short, expires: Date.now() });
    const shortLived = listingFrom(1);
    emit({
      sender: shortLived.seller,
      message: shortLived,
      expires: 1_800_000_000_000 + 1_000,
    });
    const items: string[] = [];
    for (let index = 2; index < 51; index++) {
      tick(1);
      const listing = listingFrom(index);
      items.push(listing.id);
      emit({ sender: listing.seller, message: listing });
    }
    tick(2_000);
    // The board is full; the next one replaces the one that expired.
    const next = listingFrom(51);
    emit({ sender: next.seller, message: next });
    let board = ((await seller.call({ op: "refresh" })) as Snapshot).listings;
    expect(board.map((listing) => listing.id)).toEqual([
      own,
      ...items,
      next.id,
    ]);
    // With nothing expired, the one that came first makes room.
    const newest = listingFrom(52);
    emit({ sender: newest.seller, message: newest });
    board = ((await seller.call({ op: "refresh" })) as Snapshot).listings;
    expect(board.length).toBe(51);
    expect(board[0]!.id).toBe(own);
    expect(board.map((listing) => listing.id)).not.toContain(items[0]);
    expect(board.at(-1)!.id).toBe(newest.id);
  });

  test("a seller keeps only orders its key opens for its own listings, at most 50, newest first kept", async () => {
    const { makeHost, emit, tick } = setup();
    const alice = makeHost(account("1"));
    const bob = makeHost(account("2"));
    await register(alice.connect("provider.test", "provider").call);
    const seller = bob.connect("market.test", "market");
    const buyer = alice.connect("market.test", "market");
    await seller.call({ op: "list", item: "Blue notebook" });
    const listing = ((await buyer.call({ op: "refresh" })) as Snapshot)
      .listings[0]!;
    const { order } = (await buyer.call({
      op: "buy",
      listing: listing.id,
    })) as { order: string };
    const legit = `${order}:${account("1")}`;
    const sealed = (sender: string, id: string, address: string) =>
      toHex(
        sealTo(
          fromHex(listing.key),
          utf8(address),
          toHex(
            digest(
              FIELD_DEMO,
              listing.id,
              listing.item,
              listing.seller,
              listing.key,
              sender,
              id,
              "address",
            ),
          ),
          cryptoRandom,
        ),
      );
    const purchase = (sender: string, id: string, box: string) =>
      emit({
        sender,
        message: { v: 1, type: "purchase", id, listing: listing.id, box },
      });
    // Not this seller's listing, and a box its key does not open: neither is kept.
    emit({
      sender: account("3"),
      message: {
        v: 1,
        type: "purchase",
        id: crypto.randomUUID(),
        listing: crypto.randomUUID(),
        box: "0x" + "01".repeat(100),
      },
    });
    purchase(account("3"), crypto.randomUUID(), "0x" + "01".repeat(100));
    // Another signer's valid copy of the legitimate id is another order.
    purchase(account("3"), order, sealed(account("3"), order, "Copied Street"));
    let orders = ((await seller.call({ op: "refresh" })) as Snapshot).orders;
    expect(orders.map((sale) => sale.id).sort()).toEqual(
      [legit, `${order}:${account("3")}`].sort(),
    );
    const { view } = (await seller.call({ op: "address", order: legit })) as {
      view: string;
    };
    expect(await redeem(bob.host, view, seller.child).first).toEqual({
      lines: ADDRESS_LINES,
    });
    // A flood of valid orders keeps the newest 50, dropping what came first.
    const flood: string[] = [];
    for (let index = 0; index < 60; index++) {
      tick(1);
      const sender = `0x${index.toString(16).padStart(64, "b")}`;
      const id = crypto.randomUUID();
      flood.push(`${id}:${sender}`);
      purchase(sender, id, sealed(sender, id, `Flood ${index}`));
    }
    orders = ((await seller.call({ op: "refresh" })) as Snapshot).orders;
    expect(orders.length).toBe(50);
    expect(orders.map((sale) => sale.id).sort()).toEqual(
      flood.slice(10).sort(),
    );
  });
});
