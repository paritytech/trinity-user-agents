import type { DetailSurface } from "./protocol.js";

/**
 * DRAFT, web-only. How this web host draws a protected detail: an image the
 * host's local image service serves once, for the product's origin, embedded
 * with `<img referrerpolicy="origin">`. Only {@link mountProtectedDetail}
 * reads it; a product's own code holds a surface as an opaque
 * {@link DetailSurface}.
 */
export interface WebImageSurface extends DetailSurface {
  platform: "web-image";
  url: string;
}

export const isWebImageSurface = (
  surface: DetailSurface,
): surface is WebImageSurface =>
  surface.platform === "web-image" &&
  typeof (surface as Partial<WebImageSurface>).url === "string";
