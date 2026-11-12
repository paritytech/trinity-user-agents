import { afterEach, describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import type { PromptRequest } from "../../../src/app/prompt.js";
import {
  FieldHost,
  type FieldLabel,
  type FieldSession,
  type KeyBinder,
} from "../../../src/fields/host.js";
import type {
  FieldDescriptor,
  FieldGrant,
  OpenedEnvelope,
  ReceiveKey,
} from "../../../bridges/fields/protocol.js";
import { concat, toHex, utf8 } from "../../../bridges/primitives/bytes.js";

const ADDRESS = "Ada Example\n24 Sample Lane\nExample City";
const ADDRESS_LINES = ["Ada Example", "24 Sample Lane", "Example City"];
const TRAVEL = "Ada Example\nc/o Fixture Hotel";
const PHONE = "+1 555 010 0123";
const SECRETS = ["Sample Lane", "Fixture Hotel", PHONE];
const MARKET = "market.example";
const PEOPLE = `0x${"11".repeat(32)}`;
const INDEX = `0x${"22".repeat(32)}`;
const CONTEXT = `0x${"33".repeat(32)}`;
const FIELDS = [
  { id: "ship-to", kind: "address" },
  { id: "phone", kind: "phone", optional: true },
];

/**
 * Stands in for the core's binding signature, which the Rust tests cover:
 * the account follows the wallet, product and index, and a signature checks
 * only for exactly what it was made over.
 */
function fakeBinder(wallet: string): KeyBinder {
  const sign = (product: string, account: Uint8Array, binding: Uint8Array) =>
    blake2b(concat(utf8(product), account, binding), { dkLen: 64 });
  return {
    async sign(product, index, binding) {
      const account = blake2b(concat(utf8(wallet), utf8(product), index), {
        dkLen: 32,
      });
      return { account, signature: sign(product, account, binding) };
    },
    async verify(product, account, binding, signature) {
      return toHex(sign(product, account, binding)) === toHex(signature);
    },
  };
}

const hosts: FieldHost[] = [];
const ports: MessagePort[] = [];
afterEach(() => {
  for (const host of hosts.splice(0)) host.setSession(null);
  for (const port of ports.splice(0)) port.close();
});

type Call = (request: Record<string, unknown>) => Promise<unknown>;

/** One host tab of `wallet`, over `storage`, which a restart keeps. */
function makeHost(
  wallet: string,
  storage = new Map<string, string>(),
  people = PEOPLE,
) {
  const sheets: PromptRequest<unknown>[] = [];
  const labels: FieldLabel[] = [];
  let choice: (sheet: PromptRequest<unknown>) => unknown = (sheet) =>
    sheet.choices[0]!.value;
  let onAsk: () => void = () => {};
  const host = new FieldHost({
    ask: async <T>(sheet: PromptRequest<T>) => {
      sheets.push(sheet as PromptRequest<unknown>);
      onAsk();
      return choice(sheet as PromptRequest<unknown>) as T;
    },
    binder: fakeBinder(wallet),
    storage: {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => void storage.set(key, value),
    },
    viewUrl: (token, theme) => `view:${token}?${theme}`,
    print: async (label) => void labels.push(label),
  });
  hosts.push(host);
  const session: FieldSession = {
    network: "test-net",
    people,
    walletId: wallet,
    entropy: utf8(`entropy of ${wallet}`.padEnd(16, "-")),
  };
  host.setSession(session);
  const connect = (product: string, role: "provider" | "consumer") => {
    const frame = { product };
    const channel = new MessageChannel();
    ports.push(channel.port1);
    const close = host.connect(product, role, channel.port2, () => frame);
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
    const call: Call = (request) =>
      new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject });
        channel.port1.postMessage({ ...request, id });
      });
    return { call, received, child: { parent: frame }, close };
  };
  return {
    host,
    session,
    connect,
    sheets,
    labels,
    choose: (next: typeof choice) => {
      choice = next;
    },
    whenAsked: (next: () => void) => {
      onAsk = next;
    },
  };
}

const codeOf = (promise: Promise<unknown>) =>
  promise.then(
    () => "ok",
    (error: { code?: string }) => error.code ?? "no code",
  );

async function provider(
  tab: ReturnType<typeof makeHost>,
  product: string,
  values: Record<string, string>,
) {
  const app = tab.connect(product, "provider");
  await app.call({
    op: "declare",
    name: product.split(".")[0],
    fields: ["address", "phone"],
  });
  await app.call({ op: "save", values });
  return app;
}

/** What a host-origin frame inside `owner` gets for `view`, or for another source. */
async function draw(
  tab: ReturnType<typeof makeHost>,
  surface: unknown,
  source: unknown,
): Promise<unknown> {
  const url = (surface as { url: string }).url;
  const token = url.slice("view:".length).split("?")[0];
  const channel = new MessageChannel();
  ports.push(channel.port1);
  const answer = new Promise((resolve) => {
    channel.port1.onmessage = (event) => resolve(event.data);
  });
  channel.port1.start();
  tab.host.attachView(token, source, channel.port2);
  return answer;
}

/** Alice requests to `key` from her market; answers the grant. */
async function share(
  alice: ReturnType<typeof makeHost>,
  key: ReceiveKey,
  market = alice.connect(MARKET, "consumer"),
) {
  return {
    market,
    grant: (await market.call({
      op: "request",
      fields: FIELDS,
      purpose: "Ship “Lamp” to you",
      context: CONTEXT,
      recipient: key,
    })) as FieldGrant,
  };
}

describe("the draft field bridge", () => {
  test("a provider declares and saves with no sheet, and learns only which kinds it saved", async () => {
    const alice = makeHost("alice");
    const app = await provider(alice, "notes.example", { address: ADDRESS });
    expect({
      status: await app.call({ op: "status" }),
      sheets: alice.sheets.length,
      undeclared: await codeOf(
        app.call({ op: "save", values: { email: "a@example.test" } }),
      ),
    }).toEqual({
      status: {
        registered: true,
        name: "notes",
        fields: ["address", "phone"],
        saved: ["address"],
      },
      sheets: 0,
      undeclared: "invalid",
    });
  });

  test("seals address and optional phone to a bound key; only the recipient's host opens it, into fresh handles", async () => {
    const bob = makeHost("bob");
    const bobMarket = bob.connect(MARKET, "consumer");
    const key = (await bobMarket.call({
      op: "createReceiveKey",
      index: INDEX,
    })) as ReceiveKey;
    expect(
      (
        (await bobMarket.call({
          op: "createReceiveKey",
          index: INDEX,
        })) as ReceiveKey
      ).keyId,
    ).toBe(key.keyId);

    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS, phone: PHONE });
    const { market, grant } = await share(alice, key);
    const sheet = alice.sheets[0]!;
    expect({
      title: sheet.title,
      from: sheet.from,
      choices: sheet.choices.map((choice) => choice.label),
      kinds: grant.fields.map((field) => field.kind),
      envelope: typeof grant.envelope,
    }).toEqual({
      title: "Send your address?",
      from: MARKET,
      choices: [
        `Share address: ${ADDRESS_LINES.join(", ")} · phone: ${PHONE} · from notes · notes.example`,
        `Share address: ${ADDRESS_LINES.join(", ")} · from notes · notes.example`,
        "Cancel",
      ],
      kinds: ["address", "phone"],
      envelope: "string",
    });

    const first = (await bobMarket.call({
      op: "open",
      envelope: grant.envelope,
      context: CONTEXT,
    })) as OpenedEnvelope;
    const second = (await bobMarket.call({
      op: "open",
      envelope: grant.envelope,
      context: CONTEXT,
    })) as OpenedEnvelope;
    expect({
      sameEnvelope: first.envelopeId === second.envelopeId,
      freshHandles: first.fields[0]!.handle !== second.fields[0]!.handle,
      descriptor: { ...first.fields[0], handle: "…" },
    }).toEqual({
      sameEnvelope: true,
      freshHandles: true,
      descriptor: {
        id: "ship-to",
        kind: "address",
        schema: "postal-address/1",
        handle: "…",
      },
    });

    // Another app of Bob's host, another wallet, or another context opens nothing.
    const eve = makeHost("eve");
    expect({
      otherApp: await codeOf(
        bob.connect("other.example", "consumer").call({
          op: "open",
          envelope: grant.envelope,
          context: CONTEXT,
        }),
      ),
      otherWallet: await codeOf(
        eve.connect(MARKET, "consumer").call({
          op: "open",
          envelope: grant.envelope,
          context: CONTEXT,
        }),
      ),
      otherContext: await codeOf(
        bobMarket.call({
          op: "open",
          envelope: grant.envelope,
          context: `0x${"44".repeat(32)}`,
        }),
      ),
    }).toEqual({
      otherApp: "invalid",
      otherWallet: "invalid",
      otherContext: "invalid",
    });

    const replies = JSON.stringify([market.received, bobMarket.received]);
    expect(SECRETS.filter((secret) => replies.includes(secret))).toEqual([]);
  });

  test("checks the recipient key before asking: its signature, product and network", async () => {
    const bob = makeHost("bob");
    const key = (await bob
      .connect(MARKET, "consumer")
      .call({ op: "createReceiveKey", index: INDEX })) as ReceiveKey;
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const ask = (recipient: unknown) =>
      codeOf(
        market.call({
          op: "request",
          fields: FIELDS,
          purpose: "Ship it",
          context: CONTEXT,
          recipient,
        }),
      );
    const flip = (hex: string) =>
      `${hex.slice(0, -1)}${hex.endsWith("0") ? "1" : "0"}`;
    expect({
      // A product's own key, under Bob's account, without his host's binding.
      swappedKey: await ask({ ...key, publicKey: flip(key.publicKey) }),
      otherAccount: await ask({ ...key, account: flip(key.account) }),
      badSignature: await ask({
        ...key,
        binding: { ...key.binding, signature: flip(key.binding.signature) },
      }),
      otherProduct: await ask({ ...key, product: "other.example" }),
      otherNetwork: await ask({ ...key, network: `0x${"99".repeat(32)}` }),
      manualCompare: await ask({ ...key, binding: { kind: "manual-compare" } }),
      sheets: alice.sheets.length,
    }).toEqual({
      swappedKey: "invalid",
      otherAccount: "invalid",
      badSignature: "invalid",
      otherProduct: "invalid",
      otherNetwork: "invalid",
      manualCompare: "invalid",
      sheets: 0,
    });
  });

  test("offers each provider, seals exactly what the sheet showed, and asks nothing without a provider", async () => {
    const bob = makeHost("bob");
    const bobMarket = bob.connect(MARKET, "consumer");
    const key = (await bobMarket.call({
      op: "createReceiveKey",
      index: INDEX,
    })) as ReceiveKey;
    const alice = makeHost("alice");
    expect(await codeOf(share(alice, key))).toBe("unavailable");
    expect(alice.sheets).toEqual([]);

    const home = await provider(alice, "notes.example", { address: ADDRESS });
    await provider(alice, "travel.example", { address: TRAVEL });
    alice.choose((sheet) => {
      expect(sheet.choices.map((choice) => choice.label)).toEqual([
        `Share address: ${ADDRESS_LINES.join(", ")} · from notes · notes.example`,
        "Share address: Ada Example, c/o Fixture Hotel · from travel · travel.example",
        "Cancel",
      ]);
      return sheet.choices[0]!.value;
    });
    // The provider changes its value while the sheet is open: the shown one is sealed.
    alice.whenAsked(
      () =>
        void home.call({ op: "save", values: { address: "Changed Lane 1" } }),
    );
    const { grant } = await share(alice, key);
    const opened = (await bobMarket.call({
      op: "open",
      envelope: grant.envelope,
      context: CONTEXT,
    })) as OpenedEnvelope;
    const surface = (
      (await bobMarket.call({
        op: "display",
        handle: opened.fields[0]!.handle,
      })) as { surface: unknown }
    ).surface;
    expect(await draw(bob, surface, bobMarket.child)).toEqual({
      lines: ADDRESS_LINES,
    });

    alice.whenAsked(() => {});
    alice.choose((sheet) => sheet.choices.at(-1)!.value);
    expect(await codeOf(share(alice, key))).toBe("cancelled");
  });

  test("draws a handle only in a host frame inside its own product frame, once per token, and stops after release", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const grant = (await market.call({
      op: "request",
      fields: [{ id: "ship-to", kind: "address" }],
      purpose: "Show your address on the receipt",
      context: CONTEXT,
    })) as FieldGrant;
    expect(grant.envelope).toBeUndefined();
    const handle = grant.fields[0]!.handle;
    const view = async () =>
      ((await market.call({ op: "display", handle })) as { surface: unknown })
        .surface;
    const surface = await view();
    expect(surface).toMatchObject({ platform: "web-frame" });
    expect({
      foreign: await draw(alice, surface, { parent: { product: "other" } }),
    }).toEqual({ foreign: { unavailable: true } });
    const again = await view();
    expect(await draw(alice, again, market.child)).toEqual({
      lines: ADDRESS_LINES,
    });
    expect(await draw(alice, again, market.child)).toEqual({
      unavailable: true,
    });
    await market.call({ op: "release", handles: [handle] });
    expect(await codeOf(market.call({ op: "display", handle }))).toBe(
      "unavailable",
    );
  });

  test("a restarted host opens a stored envelope again; sign-out locks it; another network's key is refused", async () => {
    const storage = new Map<string, string>();
    const bob = makeHost("bob", storage);
    const key = (await bob
      .connect(MARKET, "consumer")
      .call({ op: "createReceiveKey", index: INDEX })) as ReceiveKey;
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const { grant } = await share(alice, key);

    const restarted = makeHost("bob", storage);
    const market = restarted.connect(MARKET, "consumer");
    const open = () =>
      market.call({ op: "open", envelope: grant.envelope, context: CONTEXT });
    const opened = (await open()) as OpenedEnvelope;
    const surface = (
      (await market.call({
        op: "display",
        handle: opened.fields[0]!.handle,
      })) as { surface: unknown }
    ).surface;
    expect(await draw(restarted, surface, market.child)).toEqual({
      lines: ADDRESS_LINES,
    });

    // Sign-out ends the bridge; while signed out, nothing opens.
    restarted.host.setSession(null);
    expect(
      await codeOf(
        restarted.connect(MARKET, "consumer").call({
          op: "open",
          envelope: grant.envelope,
          context: CONTEXT,
        }),
      ),
    ).toBe("session");
    restarted.host.setSession(restarted.session);
    const back = restarted.connect(MARKET, "consumer");
    expect(
      (
        (await back.call({
          op: "open",
          envelope: grant.envelope,
          context: CONTEXT,
        })) as OpenedEnvelope
      ).fields.map((field: FieldDescriptor) => field.kind),
    ).toEqual(["address"]);

    const elsewhere = makeHost("alice", new Map(), `0x${"55".repeat(32)}`);
    await provider(elsewhere, "notes.example", { address: ADDRESS });
    expect(await codeOf(share(elsewhere, key))).toBe("invalid");
  });

  test("prints the product's lines and the held values from a host document, answering only that it printed", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS, phone: PHONE });
    const market = alice.connect(MARKET, "consumer");
    const grant = (await market.call({
      op: "request",
      fields: FIELDS,
      purpose: "Label",
      context: CONTEXT,
    })) as FieldGrant;
    expect(
      await market.call({
        op: "print",
        title: "Shipping label",
        lines: ["#ABC", "1 × Lamp"],
        handles: grant.fields.map((field) => field.handle),
      }),
    ).toEqual({ printed: true });
    expect(alice.labels).toEqual([
      {
        product: MARKET,
        title: "Shipping label",
        lines: ["#ABC", "1 × Lamp"],
        fields: [
          { kind: "address", lines: ADDRESS_LINES },
          { kind: "phone", lines: [PHONE] },
        ],
      },
    ]);
  });

  test("refuses malformed requests before asking", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const ask = (fields: unknown, extra: Record<string, unknown> = {}) =>
      codeOf(
        market.call({
          op: "request",
          fields,
          purpose: "Ship it",
          context: CONTEXT,
          ...extra,
        }),
      );
    expect({
      sameId: await ask([
        { id: "a", kind: "address" },
        { id: "a", kind: "phone" },
      ]),
      sameKind: await ask([
        { id: "a", kind: "address" },
        { id: "b", kind: "address" },
      ]),
      allOptional: await ask([{ id: "a", kind: "address", optional: true }]),
      unknownKind: await ask([{ id: "a", kind: "email" }]),
      badContext: await ask([{ id: "a", kind: "address" }], {
        context: "0x12",
      }),
      providerOp: await codeOf(
        alice.connect("notes.example", "provider").call({ op: "request" }),
      ),
      sheets: alice.sheets.length,
    }).toEqual({
      sameId: "invalid",
      sameKind: "invalid",
      allOptional: "invalid",
      unknownKind: "invalid",
      badContext: "invalid",
      providerOp: "unsupported",
      sheets: 0,
    });
  });
});
