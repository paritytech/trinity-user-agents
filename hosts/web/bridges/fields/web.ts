import type { FieldSurface } from "./protocol.js";

/**
 * DRAFT, web-only. How this web host draws a protected field: an image the
 * host's local image service serves once, for the product's origin, embedded
 * with `<img referrerpolicy="origin">`. Only {@link mountProtectedField}
 * reads it; a product's own code holds a surface as an opaque
 * {@link FieldSurface}.
 */
export interface WebImageSurface extends FieldSurface {
  platform: "web-image";
  url: string;
}

export const isWebImageSurface = (
  surface: FieldSurface,
): surface is WebImageSurface =>
  surface.platform === "web-image" &&
  typeof (surface as Partial<WebImageSurface>).url === "string";
