import { resolve } from "node:path";
import { defineConfig } from "vite";
import { aliases, envPrefix } from "./vite.shared.ts";

/** The Worker executable: one ES module, `dist/worker/index.js`. */
export default defineConfig({
  envPrefix,
  resolve: { alias: aliases },
  build: {
    outDir: resolve(import.meta.dirname, "dist/worker"),
    emptyOutDir: true,
    target: "es2022",
    lib: {
      entry: resolve(import.meta.dirname, "worker/index.ts"),
      formats: ["es"],
      fileName: () => "index.js",
    },
    rolldownOptions: {
      output: { codeSplitting: false },
    },
  },
});
