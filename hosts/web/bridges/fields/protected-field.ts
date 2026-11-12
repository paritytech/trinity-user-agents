import type { FieldClient } from "./product.js";
import type { ConsumerCalls, FieldHandle, FieldTheme } from "./protocol.js";
import { isWebFieldSurface } from "./web.js";

/**
 * DRAFT, web-only. The product-side protected field: give it a handle and a
 * place, and the host draws the value there. The product decides where and
 * how big; the host decides what. On the web the host draws in a host-origin
 * frame, so the product's scripts cannot read its text or pixels. Another
 * platform would place a native view through its own adapter behind the same
 * call.
 */
export interface ProtectedField {
  /** Stop drawing: removes the host view. Releasing the handle is separate. */
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
  if (!isWebFieldSurface(surface))
    throw new Error("This host draws fields in a way this page cannot place.");
  const frame = document.createElement("iframe");
  frame.className = "protected-field";
  frame.title = options.title;
  frame.referrerPolicy = "no-referrer";
  frame.src = surface.url;
  slot.replaceChildren(frame);
  return { remove: () => frame.remove() };
}
