import type { ImageSource } from "@parity/truapi";
import type { PocketCardKey, PocketFace, PocketFaces } from "./pocket-faces.js";
import { renderTree } from "./product-render.js";

/**
 * Bind a product card in Pocket: draw the face its product draws over the
 * bundled one, and open the card when it is pressed.
 *
 * The bundled face stays until a kept or live face arrives, and comes back
 * when a face cannot be drawn. A press on a control inside the face goes to
 * the product, not to opening the card.
 */
export interface ProductCardBinding {
  key: PocketCardKey;
  title: string;
  faces: PocketFaces;
  image(source: ImageSource): Promise<string | null> | string | null;
  open(): void;
  log(text: string): void;
}

const CONTROLS = "button, input, textarea, select, label, a";

export function bindProductCard(
  root: HTMLElement,
  bundled: HTMLElement,
  binding: ProductCardBinding,
): void {
  const { key, faces, log } = binding;
  const live = document.createElement("div");
  live.className = "pocket-live";
  root.append(live);
  root.tabIndex = 0;
  root.setAttribute("role", "button");
  root.setAttribute("aria-label", `Open ${binding.title}`);

  let drawn: PocketFace["tree"] | undefined = undefined;
  const draw = () => {
    const face = faces.face(key);
    if (face.tree === drawn) return;
    drawn = face.tree;
    root.dataset.face = face.source;
    if (face.tree === null) {
      live.replaceChildren();
      bundled.hidden = false;
      return;
    }
    try {
      live.replaceChildren(
        renderTree(face.tree, {
          action: (actionId, payload) => faces.send(key, actionId, payload),
          image: binding.image,
          unsupported: (what) =>
            log(`pocket: this host does not draw ${what} in ${key.cardId}`),
        }),
      );
      bundled.hidden = true;
    } catch (error) {
      log(
        `pocket: ${key.cardId}'s face cannot be drawn: ${error instanceof Error ? error.message : String(error)}`,
      );
      live.replaceChildren();
      bundled.hidden = false;
      root.dataset.face = "bundled";
    }
  };

  const onClick = (event: MouseEvent) => {
    if ((event.target as Element | null)?.closest(CONTROLS)) return;
    binding.open();
  };
  const onKey = (event: KeyboardEvent) => {
    if (event.target !== root) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    binding.open();
  };
  root.addEventListener("click", onClick);
  root.addEventListener("keydown", onKey);
  faces.onChange(draw);
  draw();
}
