import { afterEach, describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { x25519Pair } from "../../../src/invitations/sealing.js";
import type { PromptRequest } from "../../../src/app/prompt.js";
import {
  LocketHost,
  type LocketLabel,
  type LocketSession,
  type KeyBinder,
} from "../../../src/locket/host.js";
import type {
  DetailDescriptor,
  DetailGrant,
  OpenedEnvelope,
  RecipientKey,
} from "../../../bridges/locket/protocol.js";
import {
  concat,
  fromHex,
  toHex,
  utf8,
} from "../../../bridges/primitives/bytes.js";

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

const hosts: LocketHost[] = [];
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
  const labels: LocketLabel[] = [];
  let choice: (sheet: PromptRequest<unknown>) => unknown = (sheet) =>
    sheet.choices[0]!.value;
  let onAsk: () => void = () => {};
  let clock = 1_800_000_000_000;
  /** When "fail", the image service fails. */
  let service: "ok" | "fail" = "ok";
  /** While set, the image service waits for it before it answers. */
  let gate: Promise<void> | null = null;
  /** Images issued, by url, and whether each was revoked. */
  const images = new Map<
    string,
    { image: { product: string; svg: string }; revoked: boolean }
  >();
  const host = new LocketHost({
    images: {
      issue: async (image) => {
        if (gate !== null) await gate;
        if (service === "fail") throw new Error("service down");
        const url = `image:${images.size}`;
        const entry = { image, revoked: false };
        images.set(url, entry);
        return {
          url,
          expires: clock + 60_000,
          revoke: async () => {
            entry.revoked = true;
          },
        };
      },
    },
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
    print: async (label) => void labels.push(label),
    now: () => clock,
  });
  hosts.push(host);
  const session: LocketSession = {
    network: "test-net",
    people,
    walletId: wallet,
    entropy: utf8(`entropy of ${wallet}`.padEnd(16, "-")),
  };
  host.setSession(session);
  const connect = (product: string, role: "provider" | "consumer") => {
    const channel = new MessageChannel();
    ports.push(channel.port1);
    const close = host.connect(product, role, channel.port2);
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
    return { call, received, close };
  };
  return {
    host,
    session,
    connect,
    sheets,
    labels,
    images,
    advance: (ms: number) => {
      clock += ms;
    },
    serviceIs: (next: typeof service) => {
      service = next;
    },
    /** Hold the image service; answers how to let it go. */
    holdService: () => {
      let open!: () => void;
      gate = new Promise<void>((resolve) => (open = resolve));
      return () => {
        gate = null;
        open();
      };
    },
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
    details: ["address", "phone"],
  });
  await app.call({
    op: "setValues",
    values: { address: null, phone: null, ...values },
  });
  return app;
}

/** Whether `tab` issued `surface` for `product` as an image showing every one of `lines`. */
function shows(
  tab: ReturnType<typeof makeHost>,
  surface: unknown,
  lines: readonly string[],
  product = MARKET,
): boolean {
  const issued = tab.images.get((surface as { url: string }).url);
  return (
    (surface as { platform?: unknown }).platform === "web-image" &&
    issued?.image.product === product &&
    !issued.revoked &&
    lines.every((line) => issued.image.svg.includes(line))
  );
}

/** Alice requests to `key` from her market; answers the grant. */
async function share(
  alice: ReturnType<typeof makeHost>,
  key: RecipientKey,
  market = alice.connect(MARKET, "consumer"),
) {
  return {
    market,
    grant: (await market.call({
      op: "request",
      details: FIELDS,
      purpose: "Ship “Lamp” to you",
      context: CONTEXT,
      recipient: key,
    })) as DetailGrant,
  };
}

describe("the draft Locket bridge", () => {
  test("a provider declares and sets values with no sheet, and learns only which kinds it saved", async () => {
    const alice = makeHost("alice");
    const app = await provider(alice, "notes.example", { address: ADDRESS });
    expect({
      status: await app.call({ op: "status" }),
      sheets: alice.sheets.length,
      undeclared: await codeOf(
        app.call({ op: "setValues", values: { email: "a@example.test" } }),
      ),
    }).toEqual({
      status: {
        registered: true,
        name: "notes",
        details: ["address", "phone"],
        saved: ["address"],
      },
      sheets: 0,
      undeclared: "invalid",
    });
  });

  test("declaring again is the same declaration or invalid, and setValues replaces every value", async () => {
    const alice = makeHost("alice");
    const app = await provider(alice, "notes.example", { address: ADDRESS });
    const declare = (name: string, details: string[]) =>
      codeOf(app.call({ op: "declare", name, details }));
    const save = (values: Record<string, unknown>) =>
      codeOf(app.call({ op: "setValues", values }));
    const saved = async () =>
      ((await app.call({ op: "status" })) as { saved: string[] }).saved;
    const declarations = {
      same: await declare("notes", ["phone", "address"]),
      renamed: await declare("notes 2", ["address", "phone"]),
      fewer: await declare("notes", ["address"]),
    };
    const saves = {
      // Each setValues names every declared kind; null clears one.
      phoneOnly: await save({ address: null, phone: PHONE }),
      afterPhoneOnly: await saved(),
      missingKind: await save({ address: ADDRESS }),
      empty: await save({ address: "", phone: null }),
      extra: await save({ address: ADDRESS, phone: null, email: "a@x.test" }),
      afterRefused: await saved(),
      none: await save({ address: null, phone: null }),
      afterNone: await saved(),
    };
    expect({ declarations, saves }).toEqual({
      declarations: { same: "ok", renamed: "invalid", fewer: "invalid" },
      saves: {
        phoneOnly: "ok",
        afterPhoneOnly: ["phone"],
        missingKind: "invalid",
        empty: "invalid",
        extra: "invalid",
        afterRefused: ["phone"],
        none: "ok",
        afterNone: [],
      },
    });
  });

  test("tells a missing key from a changed envelope, an unknown version and an unreadable stored key", async () => {
    const storage = new Map<string, string>();
    const bob = makeHost("bob", storage);
    const market = bob.connect(MARKET, "consumer");
    const key = (await market.call({
      op: "recipientKey",
      index: INDEX,
    })) as RecipientKey;
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const envelope = (await share(alice, key)).grant.envelope!;
    /** `envelope` with the byte at `at` (counted in bytes) flipped. */
    const flip = (at: number) => {
      const bytes = fromHex(envelope);
      bytes[at]! ^= 1;
      return toHex(bytes);
    };
    const open = (bytes: string) =>
      codeOf(market.call({ op: "open", envelope: bytes, context: CONTEXT }));
    const codes = {
      intact: await open(envelope),
      // The key id is read before anything is checked: a changed one matches no key.
      keyId: await open(flip(5)),
      body: await open(flip(fromHex(envelope).length - 20)),
      version: await open(flip(0)),
      notHex: await open("0xzz"),
    };
    // The stored record of Bob's key, changed on this device.
    const [slot, record] = [...storage].find(([name]) =>
      name.endsWith(":accounts"),
    )!;
    const stored = JSON.parse(record) as { sealed: string }[];
    const sealed = fromHex(stored[0]!.sealed);
    sealed[0]! ^= 1;
    stored[0]!.sealed = toHex(sealed);
    storage.set(slot, JSON.stringify(stored));
    expect({ ...codes, unreadable: await open(envelope) }).toEqual({
      intact: "ok",
      keyId: "missing-key",
      body: "invalid",
      version: "unsupported",
      notHex: "invalid",
      unreadable: "unavailable",
    });
  });

  test("seals address and optional phone to a bound key; only the recipient's host opens it, into fresh refs", async () => {
    const bob = makeHost("bob");
    const bobMarket = bob.connect(MARKET, "consumer");
    const key = (await bobMarket.call({
      op: "recipientKey",
      index: INDEX,
    })) as RecipientKey;
    expect(
      (
        (await bobMarket.call({
          op: "recipientKey",
          index: INDEX,
        })) as RecipientKey
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
      kinds: grant.details.map((field) => field.kind),
      envelope: typeof grant.envelope,
    }).toEqual({
      title:
        "Share your address, and optionally your phone, with this recipient?",
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
      freshRefs: first.details[0]!.ref !== second.details[0]!.ref,
      descriptor: { ...first.details[0], ref: "…" },
    }).toEqual({
      sameEnvelope: true,
      freshRefs: true,
      descriptor: {
        id: "ship-to",
        kind: "address",
        schema: "postal-address/1",
        ref: "…",
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
      otherApp: "missing-key",
      otherWallet: "missing-key",
      otherContext: "invalid",
    });

    const replies = JSON.stringify([market.received, bobMarket.received]);
    expect(SECRETS.filter((secret) => replies.includes(secret))).toEqual([]);
  });

  test("gives each product account its own key, the same one on every call and after a restart, and opens each envelope for its own account", async () => {
    const storage = new Map<string, string>();
    const bob = makeHost("bob", storage);
    const bobMarket = bob.connect(MARKET, "consumer");
    const OTHER_INDEX = `0x${"33".repeat(32)}`;
    const keyFor = async (index: string, market = bobMarket) =>
      (await market.call({ op: "recipientKey", index })) as RecipientKey;
    const a = await keyFor(INDEX);
    const b = await keyFor(OTHER_INDEX);
    const binder = fakeBinder("bob");
    const signedBy = async (key: RecipientKey) =>
      binder.verify(
        MARKET,
        fromHex(key.account),
        concat(
          fromHex(key.keyId),
          fromHex(key.publicKey),
          fromHex(key.network),
        ),
        fromHex(key.binding.signature),
      );
    const restarted = makeHost("bob", storage);
    const restartedMarket = restarted.connect(MARKET, "consumer");
    const otherProduct = await keyFor(
      INDEX,
      bob.connect("other.example", "consumer"),
    );
    expect({
      distinct: [
        a.publicKey !== b.publicKey,
        a.keyId !== b.keyId,
        a.account !== b.account,
      ],
      signed: [await signedBy(a), await signedBy(b)],
      // A's proof does not pass as B's account.
      swapped: await signedBy({ ...b, binding: a.binding }),
      again: (await keyFor(INDEX)).keyId === a.keyId,
      afterRestart:
        (await keyFor(OTHER_INDEX, restartedMarket)).keyId === b.keyId,
      otherProduct:
        otherProduct.keyId !== a.keyId &&
        otherProduct.publicKey !== a.publicKey,
    }).toEqual({
      distinct: [true, true, true],
      signed: [true, true],
      swapped: false,
      again: true,
      afterRestart: true,
      otherProduct: true,
    });

    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const toA = (await share(alice, a)).grant.envelope;
    const toB = (await share(alice, b)).grant.envelope;
    const open = (envelope: unknown, market = restartedMarket) =>
      market.call({ op: "open", envelope, context: CONTEXT });
    expect({
      a: ((await open(toA)) as OpenedEnvelope).account,
      b: ((await open(toB)) as OpenedEnvelope).account,
    }).toEqual({ a: a.account, b: b.account });
  });

  test("still opens an envelope sealed to an earlier build's shared key, for the account it names, and never binds that key again", async () => {
    const storage = new Map<string, string>();
    const binder = fakeBinder("bob");
    const indexA = INDEX;
    const indexB = `0x${"33".repeat(32)}`;
    const accountOf = async (index: string) =>
      toHex(
        (await binder.sign(MARKET, fromHex(index), new Uint8Array(80))).account,
      );
    const accounts = [await accountOf(indexA), await accountOf(indexB)];
    // The record an earlier build kept: one key for the product, bound to both accounts.
    const entropy = utf8("entropy of bob".padEnd(16, "-"));
    const wrap = hkdf(
      sha256,
      entropy,
      utf8("test-net"),
      utf8("truapi-web-host/fields/v1/receive-key-wrap"),
      32,
    );
    const secret = new Uint8Array(32).fill(7);
    const keyId = `0x${"5a".repeat(16)}`;
    const publicKey = toHex(x25519Pair(secret).public);
    const nonce = new Uint8Array(24).fill(9);
    storage.set(
      `truapi-web-signing-host:fields:v1:test-net:bob:${MARKET}`,
      JSON.stringify({
        keyId,
        publicKey,
        created: 1,
        accounts,
        nonce: toHex(nonce),
        sealed: toHex(
          xchacha20poly1305(
            wrap,
            nonce,
            utf8(
              `web-fields/receive-key/v1|test-net|${MARKET}|${keyId}|${publicKey}`,
            ),
          ).encrypt(secret),
        ),
      }),
    );
    const legacyKey = async (
      account: string,
      index: string,
    ): Promise<RecipientKey> => {
      const network = PEOPLE;
      const signed = await binder.sign(
        MARKET,
        fromHex(index),
        concat(fromHex(keyId), fromHex(publicKey), fromHex(network)),
      );
      return {
        v: 1,
        keyId,
        publicKey,
        product: MARKET,
        account,
        network,
        binding: { kind: "host-signature", signature: toHex(signed.signature) },
      };
    };
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const toB = (await share(alice, await legacyKey(accounts[1]!, indexB)))
      .grant.envelope;
    const toA = (await share(alice, await legacyKey(accounts[0]!, indexA)))
      .grant.envelope;

    const bob = makeHost("bob", storage);
    const market = bob.connect(MARKET, "consumer");
    const opened = (await market.call({
      op: "open",
      envelope: toB,
      context: CONTEXT,
    })) as OpenedEnvelope;
    // An envelope for the first of the key's accounts keeps its own error:
    // trying the second account must not turn it into "another account".
    const wrongContext = await market
      .call({ op: "open", envelope: toA, context: `0x${"44".repeat(32)}` })
      .then(
        () => "opened",
        (error: Error) => error.message,
      );
    const fresh = (await market.call({
      op: "recipientKey",
      index: indexB,
    })) as RecipientKey;
    expect({
      wrongContext,
      forA: (
        (await market.call({
          op: "open",
          envelope: toA,
          context: CONTEXT,
        })) as OpenedEnvelope
      ).account,
      account: opened.account,
      kinds: opened.details.map((field) => field.kind),
      freshKey: fresh.keyId !== keyId && fresh.publicKey !== publicKey,
      freshAccount: fresh.account,
    }).toEqual({
      wrongContext: "This envelope is for another context.",
      forA: accounts[0],
      account: accounts[1],
      kinds: ["address"],
      freshKey: true,
      freshAccount: accounts[1],
    });
  });

  test("checks the recipient key before asking: its signature, product and network", async () => {
    const bob = makeHost("bob");
    const key = (await bob
      .connect(MARKET, "consumer")
      .call({ op: "recipientKey", index: INDEX })) as RecipientKey;
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const ask = (recipient: unknown) =>
      codeOf(
        market.call({
          op: "request",
          details: FIELDS,
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
      op: "recipientKey",
      index: INDEX,
    })) as RecipientKey;
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
        void home.call({
          op: "setValues",
          values: { address: "Changed Lane 1", phone: null },
        }),
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
        ref: opened.details[0]!.ref,
      })) as { surface: unknown }
    ).surface;
    expect(shows(bob, surface, ADDRESS_LINES)).toBe(true);

    alice.whenAsked(() => {});
    alice.choose((sheet) => sheet.choices.at(-1)!.value);
    expect(await codeOf(share(alice, key))).toBe("cancelled");
  });

  test("issues one image per display, refuses a released ref, and release and sign-out end the unused ones", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const request = async () =>
      (
        (await market.call({
          op: "request",
          details: [{ id: "ship-to", kind: "address" }],
          purpose: "Show it",
          context: CONTEXT,
        })) as DetailGrant
      ).details[0]!.ref;
    const ref = await request();
    const show = (theme?: string) => market.call({ op: "display", ref, theme });
    const first = ((await show()) as { surface: unknown }).surface;
    const dark = ((await show("dark")) as { surface: unknown }).surface;
    expect({
      first: shows(alice, first, ADDRESS_LINES),
      dark: alice.images.get("image:1")!.image.svg.includes("#17181c"),
      sameUrl: first === dark,
      replies: JSON.stringify(market.received).includes("Sample Lane"),
    }).toEqual({ first: true, dark: true, sameUrl: false, replies: false });

    await market.call({ op: "release", refs: [ref] });
    expect({
      revoked: [...alice.images.values()].map((entry) => entry.revoked),
      again: await codeOf(show()),
    }).toEqual({ revoked: [true, true], again: "missing-ref" });

    await market.call({ op: "display", ref: await request() });
    alice.host.setSession(null);
    expect(alice.images.get("image:2")?.revoked).toBe(true);
  });

  test("counts only images the service may still serve, and a failing service does not block later calls", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const ref = (
      (await market.call({
        op: "request",
        details: [{ id: "ship-to", kind: "address" }],
        purpose: "Show it",
        context: CONTEXT,
      })) as DetailGrant
    ).details[0]!.ref;
    const show = () => codeOf(market.call({ op: "display", ref }));
    for (let index = 0; index < 50; index++) await show();
    const full = await show();
    alice.advance(60_001);
    const afterExpiry = await show();
    alice.serviceIs("fail");
    const failed = await show();
    alice.serviceIs("ok");
    expect({
      full,
      afterExpiry,
      failed,
      next: await show(),
    }).toEqual({
      full: "limit",
      afterExpiry: "ok",
      failed: "unavailable",
      next: "ok",
    });
  });

  test("counts images still being issued, and frees their slots when issuing fails", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const ref = (
      (await market.call({
        op: "request",
        details: [{ id: "ship-to", kind: "address" }],
        purpose: "Show it",
        context: CONTEXT,
      })) as DetailGrant
    ).details[0]!.ref;
    const show = () => codeOf(market.call({ op: "display", ref }));
    for (let index = 0; index < 30; index++) await show();

    // A failing service, held: its 20 issues take slots, then give them back.
    alice.serviceIs("fail");
    let open = alice.holdService();
    const failing = Array.from({ length: 20 }, show);
    await new Promise((resolve) => setTimeout(resolve, 20));
    open();
    const failed = await Promise.all(failing);
    alice.serviceIs("ok");

    // 30 at once with 20 slots left: 20 are issued and 10 refused.
    open = alice.holdService();
    const racing = Array.from({ length: 30 }, show);
    await new Promise((resolve) => setTimeout(resolve, 20));
    open();
    const raced = await Promise.all(racing);
    const count = (codes: string[], code: string) =>
      codes.filter((each) => each === code).length;
    expect({
      failed: count(failed, "unavailable"),
      ok: count(raced, "ok"),
      limit: count(raced, "limit"),
      issued: alice.images.size,
    }).toEqual({ failed: 20, ok: 20, limit: 10, issued: 50 });
  });

  test("refuses calls past 32 unanswered with busy, but never a release", async () => {
    const alice = makeHost("alice");
    await provider(alice, "notes.example", { address: ADDRESS });
    const market = alice.connect(MARKET, "consumer");
    const ref = (
      (await market.call({
        op: "request",
        details: [{ id: "ship-to", kind: "address" }],
        purpose: "Show it",
        context: CONTEXT,
      })) as DetailGrant
    ).details[0]!.ref;
    const open = alice.holdService();
    const shows = Array.from({ length: 40 }, () =>
      codeOf(market.call({ op: "display", ref })),
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
    const released = await market.call({ op: "release", refs: [ref] });
    open();
    const codes = await Promise.all(shows);
    expect({
      busy: codes.filter((code) => code === "busy").length,
      // The release went first, so every display still waiting finds no ref.
      gone: codes.filter((code) => code === "missing-ref").length,
      released,
    }).toEqual({ busy: 8, gone: 32, released: { released: 1 } });
  });

  test("refuses to set a value its image cannot show", async () => {
    const alice = makeHost("alice");
    const app = alice.connect("notes.example", "provider");
    await app.call({
      op: "declare",
      name: "notes",
      details: ["address", "phone"],
    });
    const phone = await app
      .call({
        op: "setValues",
        values: { address: null, phone: "9".repeat(150) },
      })
      .then(
        () => "saved",
        (error: Error) => error.message,
      );
    expect({
      phone,
      wideAddress: await codeOf(
        app.call({
          op: "setValues",
          values: {
            address:
              "東京都架空区試験町一丁目二番三号サンプルマンション四〇五号室",
            phone: null,
          },
        }),
      ),
    }).toEqual({
      phone: "This phone is too long to show. Shorten it.",
      wideAddress: "ok",
    });
  });

  test("a restarted host opens a stored envelope again; sign-out locks it; another network's key is refused", async () => {
    const storage = new Map<string, string>();
    const bob = makeHost("bob", storage);
    const key = (await bob
      .connect(MARKET, "consumer")
      .call({ op: "recipientKey", index: INDEX })) as RecipientKey;
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
        ref: opened.details[0]!.ref,
      })) as { surface: unknown }
    ).surface;
    expect(shows(restarted, surface, ADDRESS_LINES)).toBe(true);

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
      ).details.map((field: DetailDescriptor) => field.kind),
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
      details: FIELDS,
      purpose: "Label",
      context: CONTEXT,
    })) as DetailGrant;
    expect(
      await market.call({
        op: "print",
        title: "Shipping label",
        lines: ["#ABC", "1 × Lamp"],
        refs: grant.details.map((field) => field.ref),
      }),
    ).toEqual({ printed: true });
    expect(alice.labels).toEqual([
      {
        product: MARKET,
        title: "Shipping label",
        lines: ["#ABC", "1 × Lamp"],
        details: [
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
    const ask = (details: unknown, extra: Record<string, unknown> = {}) =>
      codeOf(
        market.call({
          op: "request",
          details,
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
