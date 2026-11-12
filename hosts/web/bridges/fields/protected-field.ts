import type { FieldClient } from "./product.js";
import type { ConsumerCalls, FieldHandle, FieldTheme } from "./protocol.js";
import { isWebImageSurface } from "./web.js";

/**
 * DRAFT, web-only. The product-side protected field: give it a handle and a
 * place, and the host draws the value there. The product decides where and
 * how wide; the host decides what. On the web the host draws a cross-origin
 * SVG image, whose pixels the browser keeps from the product's scripts.
 * Another platform would place a native view through its own adapter behind
 * the same call.
 */
export interface ProtectedField {
  /** Stop drawing: removes the image. Releasing the handle is separate. */
  remove(): void;
}

/**
 * Draw `handle` inside `slot`, replacing its children. Rejects with the
 * bridge's error when the host refuses, such as for a released handle or an
 * ended session; `slot` is then left empty.
 */
export async function mountProtectedField(
  slot: HTMLElement,
  fields: Pick<FieldClient<ConsumerCalls>, "display">,
  handle: FieldHandle,
  options: { title: string; theme?: FieldTheme },
): Promise<ProtectedField> {
  slot.replaceChildren();
  const { surface } = await fields.display({
    handle,
    theme: options.theme ?? "paper",
  });
  if (!isWebImageSurface(surface))
    throw new Error("This host draws fields in a way this page cannot place.");
  const image = document.createElement("img");
  image.className = "protected-field";
  // The value is pixels only; this is all a screen reader gets.
  image.alt = `${options.title}, drawn by your host`;
  // The host serves it only to a request whose Referer is this product's origin.
  image.referrerPolicy = "origin";
  image.src = surface.url;
  slot.replaceChildren(image);
  return { remove: () => image.remove() };
}
