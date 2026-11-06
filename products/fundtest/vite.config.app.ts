import { resolve } from "node:path";
import { defineConfig } from "vite";
import { aliases, envPrefix } from "./vite.shared.ts";

const root = resolve(import.meta.dirname, "app");

/** The App executable: the provider's own screens, one page per route the
 *  worker presents with `presentFrame`. */
export default defineConfig({
  root,
  base: "./",
  envPrefix: "FUNDTEST_APP_",
  resolve: { alias: aliases },
  build: {
    outDir: resolve(import.meta.dirname, "dist/app"),
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        index: resolve(root, "index.html"),
        card: resolve(root, "card/index.html"),
        bank: resolve(root, "bank/index.html"),
        payout: resolve(root, "payout/index.html"),
      },
    },
  },
});
