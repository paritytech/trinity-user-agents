import type { BuildInfo } from "./versions.js";

declare global {
  /** Injected by `vite.config.ts` from the files this build serves. */
  const __BUILD_INFO__: BuildInfo;
}
