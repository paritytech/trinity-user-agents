import type {
  Arrangement,
  ColorToken,
  ContentAlignment,
  ImageFit,
  ImageSource,
  Modifier,
  RendererNode,
  Size,
  TypographyStyle,
} from "@parity/truapi";

/**
 * Draw a product's `RendererNode` tree as DOM.
 *
 * The tree is data, never markup: text goes in as text nodes, sizes are
 * clamped numbers, colors and type come from the host's own tokens, and an
 * image is shown only from a URL the host builds itself. A node kind or a
 * property this host does not draw is left out and reported, not guessed at.
 */

export interface RenderHooks {
  /** A button press or a text field change, with its payload. */
  action(actionId: string, payload: Uint8Array): void;
  /** The URL to show an image from, or null when the host will not show it. */
  image(source: ImageSource): Promise<string | null> | string | null;
  /** A node or property left out, once per kind. */
  unsupported?(what: string): void;
}

/** Trees deeper or larger than this are refused, not drawn in part. */
export const MAX_DEPTH = 48;
export const MAX_NODES = 2000;
const MAX_LENGTH = 2000;

const COLORS: Record<ColorToken, string> = {
  FgPrimary: "var(--chat-primary)",
  FgSecondary: "var(--chat-secondary)",
  FgTertiary: "var(--chat-tertiary)",
  BgSurfaceMain: "var(--chat-bg)",
  BgSurfaceContainer: "var(--chat-surface)",
  BgSurfaceNested: "var(--chat-surface-raised)",
  FgSuccess: "var(--chat-success)",
  FgError: "var(--chat-danger)",
  FgWarning: "var(--chat-warning)",
};

const TYPOGRAPHY: Record<TypographyStyle, string> = {
  HeadlineLarge: "pr-type-headline",
  TitleMediumRegular: "pr-type-title",
  BodyLargeRegular: "pr-type-body-large",
  BodyMediumRegular: "pr-type-body",
  BodySmallRegular: "pr-type-body-small",
};

const ARRANGEMENT: Record<Arrangement, string> = {
  Start: "flex-start",
  End: "flex-end",
  Center: "center",
  SpaceBetween: "space-between",
  SpaceAround: "space-around",
  SpaceEvenly: "space-evenly",
};

const ALIGN = {
  Start: "flex-start",
  Center: "center",
  End: "flex-end",
  Top: "flex-start",
  Bottom: "flex-end",
} as const;

const CONTENT_ALIGNMENT: Record<ContentAlignment, [string, string]> = {
  TopStart: ["start", "start"],
  TopCenter: ["start", "center"],
  TopEnd: ["start", "end"],
  CenterStart: ["center", "start"],
  Center: ["center", "center"],
  CenterEnd: ["center", "end"],
  BottomStart: ["end", "start"],
  BottomCenter: ["end", "center"],
  BottomEnd: ["end", "end"],
};

const FIT: Record<ImageFit, string> = {
  None: "none",
  Fill: "fill",
  Cover: "cover",
  Contain: "contain",
  ScaleDown: "scale-down",
};

const px = (value: Size, max = MAX_LENGTH): string =>
  `${Math.min(Math.max(Number(value) || 0, 0), max)}px`;

class TreeTooLarge extends Error {}

function applyModifiers(
  element: HTMLElement,
  modifiers: Modifier[],
  unsupported: (what: string) => void,
): void {
  const style = element.style;
  for (const modifier of modifiers) {
    switch (modifier.tag) {
      case "Margin":
      case "Padding": {
        const { top, end, bottom, start } = modifier.value;
        const value = `${px(top, 512)} ${px(end, 512)} ${px(bottom ?? top, 512)} ${px(start ?? end, 512)}`;
        if (modifier.tag === "Margin") style.margin = value;
        else style.padding = value;
        break;
      }
      case "Background":
        style.background = COLORS[modifier.value.color];
        applyShape(style, modifier.value.shape);
        break;
      case "Border":
        style.border = `${px(modifier.value.width, 16)} solid ${COLORS[modifier.value.color]}`;
        applyShape(style, modifier.value.shape);
        break;
      case "Height":
        style.height = px(modifier.value);
        break;
      case "Width":
        style.width = px(modifier.value);
        break;
      case "MinWidth":
        style.minWidth = px(modifier.value);
        break;
      case "MinHeight":
        style.minHeight = px(modifier.value);
        break;
      case "FillWidth":
        if (modifier.value) {
          style.width = "100%";
          style.alignSelf = "stretch";
        }
        break;
      case "FillHeight":
        if (modifier.value) style.height = "100%";
        break;
      case "Opacity":
        style.opacity = String(
          Math.min(Math.max(modifier.value, 0), 255) / 255,
        );
        break;
      case "BlendingMode":
        style.mixBlendMode = modifier.value
          .replace(/([a-z])([A-Z])/g, "$1-$2")
          .toLowerCase();
        break;
      default:
        unsupported(`modifier ${(modifier as { tag: string }).tag}`);
    }
  }
}

function applyShape(
  style: CSSStyleDeclaration,
  shape:
    | { tag: "Rounded"; value: Size }
    | { tag: "Circle" }
    | { tag: "Square" }
    | undefined,
): void {
  if (shape === undefined) return;
  if (shape.tag === "Rounded") style.borderRadius = px(shape.value, 512);
  else if (shape.tag === "Circle") style.borderRadius = "50%";
  else style.borderRadius = "0";
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  return node;
}

/**
 * The DOM for `root`. Throws when the tree is deeper than {@link MAX_DEPTH}
 * or has more than {@link MAX_NODES} nodes, so a caller shows an error, not
 * part of a card.
 */
export function renderTree(
  root: RendererNode,
  hooks: RenderHooks,
): HTMLElement {
  let count = 0;
  const reported = new Set<string>();
  const unsupported = (what: string) => {
    if (reported.has(what)) return;
    reported.add(what);
    hooks.unsupported?.(what);
  };

  const draw = (node: RendererNode, depth: number): Node | null => {
    count += 1;
    if (count > MAX_NODES)
      throw new TreeTooLarge(`the card has more than ${MAX_NODES} nodes`);
    if (depth > MAX_DEPTH)
      throw new TreeTooLarge(`the card is nested deeper than ${MAX_DEPTH}`);
    const children = (nodes: RendererNode[], into: HTMLElement) => {
      for (const child of nodes) {
        const drawn = draw(child, depth + 1);
        if (drawn !== null) into.append(drawn);
      }
    };
    switch (node.tag) {
      case "Nil":
        return null;
      case "String":
        return document.createTextNode(node.value.text);
      case "Column":
      case "Row": {
        const box = element(
          "div",
          node.tag === "Column" ? "pr-column" : "pr-row",
        );
        if (node.tag === "Column") {
          const { horizontalAlignment, verticalArrangement } = node.value.props;
          if (horizontalAlignment)
            box.style.alignItems = ALIGN[horizontalAlignment];
          if (verticalArrangement)
            box.style.justifyContent = ARRANGEMENT[verticalArrangement];
        } else {
          const { verticalAlignment, horizontalArrangement } = node.value.props;
          if (verticalAlignment)
            box.style.alignItems = ALIGN[verticalAlignment];
          if (horizontalArrangement)
            box.style.justifyContent = ARRANGEMENT[horizontalArrangement];
        }
        applyModifiers(box, node.value.modifiers, unsupported);
        children(node.value.children, box);
        return box;
      }
      case "Box": {
        const box = element("div", "pr-box");
        const alignment = node.value.props.contentAlignment;
        if (alignment) {
          const [block, inline] = CONTENT_ALIGNMENT[alignment];
          box.style.alignItems = block;
          box.style.justifyItems = inline;
        }
        applyModifiers(box, node.value.modifiers, unsupported);
        children(node.value.children, box);
        return box;
      }
      case "Spacer": {
        const spacer = element("div", "pr-spacer");
        applyModifiers(spacer, node.value.modifiers, unsupported);
        return spacer;
      }
      case "Text": {
        const text = element(
          "div",
          `pr-text ${TYPOGRAPHY[node.value.props.style ?? "BodyMediumRegular"]}`,
        );
        if (node.value.props.color)
          text.style.color = COLORS[node.value.props.color];
        applyModifiers(text, node.value.modifiers, unsupported);
        children(node.value.children, text);
        return text;
      }
      case "Button": {
        const {
          text,
          variant = "Primary",
          enabled,
          loading,
          clickAction,
        } = node.value.props;
        const button = element(
          "button",
          `pr-button pr-button-${variant.toLowerCase()}`,
        );
        button.type = "button";
        button.textContent = text;
        children(node.value.children, button);
        button.disabled =
          enabled === false || loading === true || clickAction === undefined;
        if (loading === true) button.setAttribute("aria-busy", "true");
        if (clickAction !== undefined)
          button.addEventListener("click", () =>
            hooks.action(clickAction, new Uint8Array()),
          );
        applyModifiers(button, node.value.modifiers, unsupported);
        return button;
      }
      case "TextField": {
        const { text, placeholder, label, enabled, valueChangeAction } =
          node.value.props;
        const field = element("label", "pr-field");
        if (label) {
          const caption = element("span", "pr-field-label");
          caption.textContent = label;
          field.append(caption);
        }
        const input = element("input", "pr-input");
        input.value = text;
        if (placeholder) input.placeholder = placeholder;
        input.disabled = enabled === false;
        if (valueChangeAction !== undefined)
          input.addEventListener("input", () =>
            hooks.action(
              valueChangeAction,
              new TextEncoder().encode(input.value),
            ),
          );
        field.append(input);
        applyModifiers(field, node.value.modifiers, unsupported);
        return field;
      }
      case "Image": {
        const image = element("img", "pr-image");
        image.alt = "";
        image.decoding = "async";
        image.referrerPolicy = "no-referrer";
        image.style.objectFit = FIT[node.value.props.fit ?? "Fill"];
        applyModifiers(image, node.value.modifiers, unsupported);
        void Promise.resolve(hooks.image(node.value.props.source)).then(
          (url) => {
            if (url !== null) image.src = url;
          },
        );
        return image;
      }
      case "Effect": {
        const effect = element(
          "div",
          `pr-effect pr-effect-${node.value.props.effect.toLowerCase()}`,
        );
        children(node.value.children, effect);
        return effect;
      }
      default:
        unsupported(`node ${(node as { tag: string }).tag}`);
        return null;
    }
  };

  const root_ = element("div", "pr-root");
  const drawn = draw(root, 0);
  if (drawn !== null) root_.append(drawn);
  return root_;
}

const BULLETIN_CID = /^b[a-z2-7]{20,120}$/;

/** The gateway URL of a Bulletin image, or null for a CID this host does not accept. */
export function bulletinImageUrl(cid: string, gateway: string): string | null {
  if (!BULLETIN_CID.test(cid)) return null;
  return `${gateway.replace(/\/+$/, "")}/ipfs/${cid}`;
}
