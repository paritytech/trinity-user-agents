/** The trees of the `Custom` message and of the `Custom` footer. */
import type { RendererNode } from "@parity/truapi";

export const CARD_LIKE_ACTION = "card-like";
export const FOOTER_PING_ACTION = "footer-ping";

type TextStyle = "HeadlineLarge" | "BodyMediumRegular" | "BodySmallRegular";

function column(children: RendererNode[]): RendererNode {
  return {
    tag: "Column",
    value: {
      modifiers: [],
      props: { horizontalAlignment: "Start", verticalArrangement: "Start" },
      children,
    },
  };
}

function text(value: string, style: TextStyle): RendererNode {
  return {
    tag: "Text",
    value: { modifiers: [], props: { style }, children: [{ tag: "String", value: { text: value } }] },
  };
}

function button(label: string, clickAction: string): RendererNode {
  return {
    tag: "Button",
    value: {
      modifiers: [],
      props: { text: label, variant: "Secondary", enabled: true, loading: false, clickAction },
      children: [],
    },
  };
}

/** The tree of a `chat-v2-card` message: a title, its counter, a button. */
export function cardTree(counter: string): RendererNode {
  return column([
    text("Sample card", "HeadlineLarge"),
    text(`Card ${counter}`, "BodyMediumRegular"),
    button("Like", CARD_LIKE_ACTION),
  ]);
}

/** The tree of the custom footer: one line of text and a button. */
export function footerTree(): RendererNode {
  return column([text("The sample product draws this footer", "BodySmallRegular"), button("Ping", FOOTER_PING_ACTION)]);
}
