import { resolve } from "node:path";
import { defineConfig } from "vite";
import { define } from "./vite.config.shared.ts";

// The launchers and `bulletin-deploy` look for the Worker at
// `out/worker/index.js`, nested in the App output.
export default defineConfig({
  root: "worker",
  define,
  build: {
    outDir: resolve("out/worker"),
    emptyOutDir: true,
    lib: {
      entry: "index.ts",
      formats: ["es"],
      fileName: "index",
    },
  },
});
