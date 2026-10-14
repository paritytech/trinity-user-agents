/** Vite replaces this name with the product's DotNS name at build time. */
declare const __CHAT_V2_PRODUCT__: string | undefined;

/** Vite inlines a `?inline` import as a `data:` URL. */
declare module "*?inline" {
  const dataUrl: string;
  export default dataUrl;
}
