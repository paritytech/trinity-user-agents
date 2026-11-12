import { afterEach, describe, expect, test } from "bun:test";
import type { PromptRequest } from "../../../src/app/prompt.js";
import { FieldHost } from "../../../src/fields/host.js";
import { fieldClient } from "../../../bridges/fields/product.js";
import {
  FieldError,
  type FieldGrant,
} from "../../../bridges/fields/protocol.js";
import { cryptoRandom, utf8 } from "../../../bridges/primitives/bytes.js";

/**
 * What happens to a call the product stops waiting for: the client's own
 * timeout, made short here instead of the 180 seconds a product waits, and
 * the `{ cancel: id }` it sends. Sheets are held open by hand, as a user who
 * has not answered yet.
 */

const ADDRESS = "Ada Example\n24 Sample Lane";
const MARKET = "market.example";
const CONTEXT = `0x${"33".repeat(32)}`;
const ADDRESS_ONLY = [{ id: "ship-to", kind: "address" as const }];

const ports: MessagePort[] = [];
const hosts: FieldHost[] = [];
afterEach(() => {
  for (const host of hosts.splice(0)) host.setSession(null);
  for (const port of ports.splice(0)) port.close();
});

/** A sheet the test answers when it chooses. */
interface OpenSheet {
  sheet: PromptRequest<unknown>;
  /** Answer it, even after it was closed, as a late tap would. */
  answer(value: unknown): void;
}

function setup() {
  const sheets: OpenSheet[] = [];
  let answerAtOnce = true;
  let randomCalls = 0;
  const stored = new Map<string, string>();
  /** While set, signing a binding waits for it. */
  let signGate: Promise<void> | null = null;
  const host = new FieldHost({
    ask: <T>(sheet: PromptRequest<T>) =>
      new Promise<T>((resolve) => {
        if (answerAtOnce) return resolve(sheet.choices[0]!.value);
        sheets.push({
          sheet: sheet as PromptRequest<unknown>,
          answer: (value) => resolve(value as T),
        });
      }),
    binder: {
      sign: async () => {
        if (signGate !== null) await signGate;
        return {
          account: new Uint8Array(32).fill(1),
          signature: new Uint8Array(64),
        };
      },
      verify: async () => true,
    },
    images: {
      issue: async () => ({
        url: "image:0",
        expires: Date.now() + 60_000,
        revoke: async () => {},
      }),
    },
    storage: {
      getItem: (key) => stored.get(key) ?? null,
      setItem: (key, value) => void stored.set(key, value),
    },
    print: async () => {},
    random: (length) => {
      randomCalls += 1;
      return cryptoRandom(length);
    },
  });
  hosts.push(host);
  host.setSession({
    network: "test-net",
    people: `0x${"11".repeat(32)}`,
    walletId: "alice",
    entropy: utf8("entropy of alice"),
  });
  const portTo = (product: string, role: "provider" | "consumer") => {
    const channel = new MessageChannel();
    ports.push(channel.port1);
    host.connect(product, role, channel.port2);
    return channel.port1;
  };
  return {
    sheets,
    portTo,
    randomCalls: () => randomCalls,
    stored,
    holdSigning: () => {
      let open!: () => void;
      signGate = new Promise<void>((resolve) => (open = resolve));
      return () => {
        signGate = null;
        open();
      };
    },
    holdSheets: () => {
      answerAtOnce = false;
    },
  };
}

/** Every answer `port` receives, as they arrive: what carries an id. */
type Answer = { id: number; result?: unknown; error?: unknown };
function received(port: MessagePort): Answer[] {
  const messages: Answer[] = [];
  port.addEventListener("message", (event: MessageEvent) => {
    if (typeof (event.data as { id?: unknown }).id === "number")
      messages.push(event.data as Answer);
  });
  return messages;
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
const codeOf = (promise: Promise<unknown>) =>
  promise.then(
    () => "ok",
    (error: unknown) =>
      error instanceof FieldError ? error.code : String(error),
  );

async function withAddress(tab: ReturnType<typeof setup>) {
  const provider = fieldClient(
    tab.portTo("notes.example", "provider"),
    "provider",
  );
  await provider.declare({ name: "notes", fields: ["address", "phone"] });
  await provider.save({ values: { address: ADDRESS, phone: null } });
}

describe("calls the product stops waiting for", () => {
  test("a request that times out closes its sheet, a late approval seals and holds nothing, and the next request works", async () => {
    const tab = setup();
    await withAddress(tab);
    tab.holdSheets();
    const port = tab.portTo(MARKET, "consumer");
    const replies = received(port);
    const fields = fieldClient(port, "consumer", { timeoutMs: 50 });
    const outcome = await codeOf(
      fields.request({
        fields: ADDRESS_ONLY,
        purpose: "Ship it",
        context: CONTEXT,
      }),
    );
    const [sheet] = tab.sheets;
    const before = tab.randomCalls();
    sheet!.answer(0);
    await settle();
    expect({
      outcome,
      sheetClosed: sheet!.sheet.signal?.aborted,
      // No handle was made and nothing was sealed after the answer.
      randomAfterLateAnswer: tab.randomCalls() - before,
      replies,
    }).toEqual({
      outcome: "timeout",
      sheetClosed: true,
      randomAfterLateAnswer: 0,
      replies: [
        {
          id: 1,
          error: {
            code: "timeout",
            message: "The app stopped waiting for this.",
          },
        },
      ],
    });

    const next = fields.request({
      fields: ADDRESS_ONLY,
      purpose: "Ship it",
      context: CONTEXT,
    });
    await settle();
    tab.sheets[1]!.answer(0);
    expect(
      ((await next) as FieldGrant).fields.map((field) => field.kind),
    ).toEqual(["address"]);
  });

  test("display and release answer while a sheet is open, and a queued request that is withdrawn never shows its sheet", async () => {
    const tab = setup();
    await withAddress(tab);
    const port = tab.portTo(MARKET, "consumer");
    const replies = received(port);
    const fields = fieldClient(port, "consumer");
    const grant = await fields.request({
      fields: ADDRESS_ONLY,
      purpose: "Show it",
      context: CONTEXT,
    });
    const handle = grant.fields[0]!.handle;
    tab.holdSheets();
    // Raw calls on the same port, with ids the client does not use.
    const send = (message: Record<string, unknown>) =>
      port.postMessage(message);
    const request = {
      op: "request",
      fields: ADDRESS_ONLY,
      purpose: "Again",
      context: CONTEXT,
    };
    send({ ...request, id: 101 });
    send({ op: "display", handle, theme: "paper", id: 102 });
    send({ op: "release", handles: [handle], id: 103 });
    send({ ...request, id: 104 });
    send({ cancel: 104 });
    await settle();
    const ids = () => replies.map((reply) => reply.id).filter((id) => id > 100);
    const whileOpen = { replied: ids(), sheets: tab.sheets.length };
    tab.sheets[0]!.answer(0);
    await settle();
    expect({
      whileOpen,
      afterAnswer: { replied: ids(), sheets: tab.sheets.length },
    }).toEqual({
      // The withdrawn request is answered at once and never asks.
      whileOpen: { replied: [102, 103, 104], sheets: 1 },
      afterAnswer: { replied: [102, 103, 104, 101], sheets: 1 },
    });
  });

  test("an answer the host sent before it saw the cancel is the call's answer, not an orphan", async () => {
    const tab = setup();
    await withAddress(tab);
    tab.holdSheets();
    const port = tab.portTo(MARKET, "consumer");
    const replies = received(port);
    // Registered before the client's own timer, so the user answers just
    // before the client gives up: the grant is on its way when the cancel goes.
    setTimeout(() => tab.sheets[0]!.answer(0), 50);
    const fields = fieldClient(port, "consumer", { timeoutMs: 50 });
    const grant = await fields.request({
      fields: ADDRESS_ONLY,
      purpose: "Ship it",
      context: CONTEXT,
    });
    await settle();
    expect({
      kinds: grant.fields.map((field) => field.kind),
      answers: replies.map((reply) => reply.id),
    }).toEqual({ kinds: ["address"], answers: [1] });
  });

  test("a key withdrawn while it is signed is not stored, and calls for one index at once make one key", async () => {
    const tab = setup();
    const port = tab.portTo(MARKET, "consumer");
    const INDEX = `0x${"0a".repeat(32)}`;
    let open = tab.holdSigning();
    const fields = fieldClient(port, "consumer", { timeoutMs: 30 });
    const withdrawn = await codeOf(fields.createReceiveKey({ index: INDEX }));
    open();
    await settle();
    const storedAfterCancel = tab.stored.size;

    open = tab.holdSigning();
    const patient = fieldClient(port, "consumer");
    const racing = Array.from({ length: 5 }, () =>
      patient.createReceiveKey({ index: INDEX }),
    );
    await settle();
    open();
    const keys = await Promise.all(racing);
    const records = JSON.parse([...tab.stored.values()][0]!) as unknown[];
    expect({
      withdrawn,
      storedAfterCancel,
      keyIds: new Set(keys.map((key) => key.keyId)).size,
      records: records.length,
    }).toEqual({
      withdrawn: "timeout",
      storedAfterCancel: 0,
      keyIds: 1,
      records: 1,
    });
  });

  test("the user's own Cancel stays cancelled, apart from a timeout", async () => {
    const tab = setup();
    await withAddress(tab);
    tab.holdSheets();
    const fields = fieldClient(tab.portTo(MARKET, "consumer"), "consumer");
    const asked = codeOf(
      fields.request({
        fields: ADDRESS_ONLY,
        purpose: "Ship it",
        context: CONTEXT,
      }),
    );
    await settle();
    tab.sheets[0]!.answer(null);
    expect(await asked).toBe("cancelled");
  });
});
