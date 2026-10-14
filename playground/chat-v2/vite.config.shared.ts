/**
 * The DotNS name of the product. The App and the Worker use it in the room
 * links and in the deeplink to the App. It must equal the `domain` of
 * `bulletin-deploy.config.ts`.
 */
export const productName =
  process.env.CHAT_V2_DOTNS_NAME ?? "truapi-chat-v2.paseo";

/** The build-time constants of the App and the Worker. */
export const define = {
  __CHAT_V2_PRODUCT__: JSON.stringify(productName),
};
