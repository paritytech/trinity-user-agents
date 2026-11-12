import type { FieldSurface } from "./protocol.js";

/**
 * DRAFT, web-only. How this web host draws a protected field: a host-origin
 * frame the product places, with a single-use ticket in its address. Only
 * {@link mountProtectedField} reads it; a product's own code holds the
 * surface as an opaque {@link FieldSurface}.
 */
export interface WebFieldSurface extends FieldSurface {
  platform: "web-frame";
  /** The host page to embed. Valid once, for a short time, inside the frame that asked. */
  url: string;
}

export const isWebFieldSurface = (
  surface: FieldSurface,
): surface is WebFieldSurface =>
  surface.platform === "web-frame" &&
  typeof (surface as Partial<WebFieldSurface>).url === "string";
