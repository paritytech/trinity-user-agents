/**
 * A product worker for the browser test of Pocket: it draws the `humanity`
 * card's face once its renderer is up, counts presses on the face's button,
 * and leaves a value in its product's storage for the product's Widget to
 * read. It starts drawing a moment after it loads, as a real worker does once
 * it has booted. What it saw is kept on `window.__pocketWorker`.
 */
import type { RendererNode } from "@parity/truapi";
import { getClientSync } from "@parity/truapi/sandbox";

interface Seen {
  account: string | null;
  cards: string[];
  renders: string[];
  actions: string[];
  errors: string[];
}

const seen: Seen = {
  account: null,
  cards: [],
  renders: [],
  actions: [],
  errors: [],
};
(window as unknown as { __pocketWorker: Seen }).__pocketWorker = seen;

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

const BOOT_MS = 1_500;
let taps = 0;

const text = (value: string): RendererNode => ({
  tag: "Text",
  value: {
    modifiers: [],
    props: { style: "TitleMediumRegular", color: "FgPrimary" },
    children: [{ tag: "String", value: { text: value } }],
  },
});
const face = (): RendererNode => ({
  tag: "Column",
  value: {
    modifiers: [
      { tag: "FillWidth", value: true },
      { tag: "Padding", value: { top: 16, end: 16 } },
    ],
    props: { verticalArrangement: "SpaceBetween" },
    children: [
      text(`fixture face, taps ${taps}`),
      {
        tag: "Button",
        value: {
          modifiers: [],
          props: { text: "tap", clickAction: "tap" },
          children: [],
        },
      },
    ],
  },
});

const account = await client.account.getAccount({
  productAccountId: {
    dotNsIdentifier: "peopl.paseo",
    derivationIndex: { tag: "Index", value: 0 },
  },
});
if (account.isOk()) seen.account = account.value.account.publicKey;
else seen.errors.push(`account: ${JSON.stringify(account.error)}`);

const written = await client.localStorage.write({
  key: "fixture.from",
  value: "0x776f726b6572",
});
if (written.isErr())
  seen.errors.push(`storage: ${JSON.stringify(written.error)}`);

client.pocket.listSubscribe().subscribe({
  next(item) {
    seen.cards = item.cards.map(
      (card) => `${card.cardId}:${card.privileged ? "pinned" : "added"}`,
    );
  },
});

setTimeout(() => {
  const redraws = new Set<() => void>();
  client.renderer.onRender((request, send) => {
    if (request.context.tag !== "PocketCard") return () => {};
    seen.renders.push(request.context.value.cardId);
    const draw = () => send(face());
    redraws.add(draw);
    draw();
    return () => redraws.delete(draw);
  });
  client.renderer.actionSubscribe().subscribe({
    next(item) {
      seen.actions.push(`${item.context.tag}:${item.actionId}`);
      if (item.actionId !== "tap") return;
      taps += 1;
      for (const draw of redraws) draw();
    },
  });
}, BOOT_MS);
