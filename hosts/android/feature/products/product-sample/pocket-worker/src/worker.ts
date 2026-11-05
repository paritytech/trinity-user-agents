// A Pocket demo worker built on the core's own client, so it needs no product SDK.
//
// It publishes one card, `loyalty`, draws a live front and back for it through the
// unified renderer, counts stamps pressed on each side, reports its card collection,
// and gives the card up when its own Remove button is pressed.
import { scale } from "@parity/truapi";
import { getClientSync } from "@parity/truapi/sandbox";
import code from "../faces/code.png";
import type {
  HostPocketListSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
  RendererNode,
} from "@parity/truapi";

const CARD_ID = "loyalty";
const STAMP_ACTION = "stamp";
const REMOVE_ACTION = "remove";
const STAMP_BACK_ACTION = "stamp-back";
const CODE = scale.bytesToHex(code);

const client = getClientSync();
if (!client) {
  throw new Error("The Pocket demo worker needs a TrUAPI host connection");
}

let stamps = 0;
let backStamps = 0;
const openFronts = new Set<(node: RendererNode) => void>();
const openBacks = new Set<(node: RendererNode) => void>();

function text(value: string, style: "TitleMediumRegular" | "BodyMediumRegular", color: "FgPrimary" | "FgSecondary"): RendererNode {
  return {
    tag: "Text",
    value: { modifiers: [], props: { style, color }, children: [{ tag: "String", value: { text: value } }] },
  };
}

function button(label: string, action: string, variant: "Primary" | "Text"): RendererNode {
  return {
    tag: "Button",
    value: { modifiers: [], props: { text: label, variant, enabled: true, loading: false, clickAction: action }, children: [] },
  };
}

function face(): RendererNode {
  return {
    tag: "Column",
    value: {
      modifiers: [
        { tag: "FillWidth", value: true },
        { tag: "Padding", value: { top: 16, end: 16 } },
      ],
      props: { verticalArrangement: "SpaceBetween" },
      children: [
        text("FRONT", "BodyMediumRegular", "FgSecondary"),
        text("Loyalty", "TitleMediumRegular", "FgPrimary"),
        text(`Stamps: ${stamps} · back stamps: ${backStamps}`, "BodyMediumRegular", "FgSecondary"),
        {
          tag: "Row",
          value: {
            modifiers: [],
            props: {},
            children: [button("Stamp", STAMP_ACTION, "Primary"), button("Remove", REMOVE_ACTION, "Text")],
          },
        },
      ],
    },
  };
}

// The back carries the same code as faces/loyalty-back.json, sent as an inline image so the
// static back shows an archive image and the live back an inline one.
function back(): RendererNode {
  return {
    tag: "Row",
    value: {
      modifiers: [
        { tag: "FillWidth", value: true },
        { tag: "Padding", value: { top: 16, end: 16 } },
      ],
      props: {},
      children: [
        {
          tag: "Image",
          value: {
            modifiers: [
              { tag: "Width", value: 120 },
              { tag: "Height", value: 120 },
            ],
            props: { source: { tag: "Inline", value: CODE }, fit: "Contain" },
          },
        },
        {
          tag: "Column",
          value: {
            modifiers: [{ tag: "Padding", value: { top: 0, end: 0, start: 16 } }],
            props: {},
            children: [
              text("BACK · live", "TitleMediumRegular", "FgPrimary"),
              text(`Back stamps: ${backStamps}`, "BodyMediumRegular", "FgSecondary"),
              button("Stamp back", STAMP_BACK_ACTION, "Primary"),
            ],
          },
        },
      ],
    },
  };
}

function redraw(): void {
  const front = face();
  for (const send of openFronts) send(front);
  const tree = back();
  for (const send of openBacks) send(tree);
}

// The host opens one render per face on screen and closes it when the face leaves.
client.renderer.onRender((request: ProductRendererRenderRequest, send: (node: RendererNode) => void) => {
  const { context } = request;
  const side =
    context.tag === "PocketCard" && context.value.cardId === CARD_ID
      ? openFronts
      : context.tag === "PocketCardBack" && context.value.cardId === CARD_ID
        ? openBacks
        : undefined;
  if (side === undefined) {
    throw new Error(`unsupported render context: ${JSON.stringify(context)}`);
  }
  side.add(send);
  send(side === openFronts ? face() : back());
  return () => {
    side.delete(send);
  };
});

client.renderer.actionSubscribe().subscribe({
  next(action: HostRendererActionSubscribeItem) {
    if (action.context.tag === "PocketCardBack") {
      if (action.actionId === STAMP_BACK_ACTION) {
        backStamps += 1;
        redraw();
      }
      return;
    }
    if (action.context.tag !== "PocketCard") return;
    if (action.actionId === STAMP_ACTION) {
      stamps += 1;
      redraw();
    }
    if (action.actionId === REMOVE_ACTION) {
      void client.pocket.removeCard({ cardId: CARD_ID }).then((outcome) => {
        console.log("Pocket demo: remove_card", outcome.isOk() ? "ok" : JSON.stringify(outcome.error));
      });
    }
  },
  error(reason: unknown) {
    console.error("Pocket demo: renderer action stream failed", reason);
  },
});

client.pocket.listSubscribe().subscribe({
  next(item: HostPocketListSubscribeItem) {
    const cards = item.cards.map((card) => (card.privileged ? `${card.cardId} (pinned)` : card.cardId));
    console.log("Pocket demo: cards", cards.length === 0 ? "none" : cards.join(", "));
  },
  error(reason: unknown) {
    console.error("Pocket demo: card list failed", reason);
  },
});
