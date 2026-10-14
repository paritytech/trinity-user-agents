// A Pocket demo worker built on the core's own client, so it needs no product SDK.
//
// It publishes one card, `loyalty`, draws a live face for it through the unified
// renderer, counts stamps pressed on that face, reports its card collection, and
// gives the card up when its own Remove button is pressed.
import { getClientSync } from "@parity/truapi/sandbox";
import type {
  HostPocketListSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
  RendererNode,
} from "@parity/truapi";

const CARD_ID = "loyalty";
const STAMP_ACTION = "stamp";
const REMOVE_ACTION = "remove";

const client = getClientSync();
if (!client) {
  throw new Error("The Pocket demo worker needs a TrUAPI host connection");
}

let stamps = 0;
const openFaces = new Set<(node: RendererNode) => void>();

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
        text("Loyalty", "TitleMediumRegular", "FgPrimary"),
        text(`Stamps: ${stamps}`, "BodyMediumRegular", "FgSecondary"),
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

function redraw(): void {
  const tree = face();
  for (const send of openFaces) send(tree);
}

// The host opens one render per face on screen and closes it when the face leaves.
client.renderer.onRender((request: ProductRendererRenderRequest, send: (node: RendererNode) => void) => {
  if (request.context.tag !== "PocketCard" || request.context.value.cardId !== CARD_ID) {
    throw new Error(`unsupported render context: ${JSON.stringify(request.context)}`);
  }
  openFaces.add(send);
  send(face());
  return () => {
    openFaces.delete(send);
  };
});

client.renderer.actionSubscribe().subscribe({
  next(action: HostRendererActionSubscribeItem) {
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
