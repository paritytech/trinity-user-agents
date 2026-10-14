import { execSync } from "node:child_process";

/**
 * Product manifest of the Chat Extension v2 sample product.
 *
 * The Worker stays nested at `out/worker`, as in the TrUAPI Playground: a
 * host can find the chat entry point by a probe for `worker/index.js` in the
 * App archive. `domain` must equal the name the CLI gets, and the name the
 * build used (`CHAT_V2_DOTNS_NAME`).
 */

// Short commit hash of the deployed tree, in the fourth element of the
// version; 'dev' when git is not available.
const buildId = (() => {
  try {
    return execSync("git rev-parse --short HEAD", { encoding: "utf8" }).trim();
  } catch {
    return "dev";
  }
})();

const appVersion = [0, 0, 1, buildId];

const manifest = {
  domain: process.env.CHAT_V2_DOTNS_NAME ?? "truapi-chat-v2.paseo",
  displayName: "Chat Extension v2 Sample",
  description:
    "Exercises every Chat Extension v2 method in a host-store and a product-source room.",
  icon: { path: "assets/avatar.png", format: "png" },
  executables: [
    { kind: "app", path: "out", appVersion },
    {
      kind: "worker",
      path: "out/worker",
      appVersion,
      entrypoint: "index.js",
      includes: { chat: true, pocket: false },
    },
  ],
};

export default manifest;
