import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";
import { define } from "./vite.config.shared.ts";

/**
 * The development bridge of `truapi-host dev`. It is a development-only
 * script, so the plugin adds it to the served page and never to the build.
 */
const DEV_BRIDGE_URL = "http://127.0.0.1:9955/bootstrap.js";

function devBridge(): Plugin {
  return {
    name: "chat-v2-dev-bridge",
    apply: "serve",
    transformIndexHtml() {
      return [{ tag: "script", attrs: { src: DEV_BRIDGE_URL }, injectTo: "head-prepend" }];
    },
  };
}

export default defineConfig({
  root: "app",
  base: "./",
  define,
  plugins: [devBridge()],
  server: { port: 3102, strictPort: true },
  build: {
    outDir: resolve("out"),
    emptyOutDir: true,
  },
});
