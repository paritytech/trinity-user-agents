import type { BuildInfo } from "./app/versions.js";
import type { ContentSeed } from "./products/seed.js";

declare global {
  /** Injected by `vite.config.ts` from the files this build serves. */
  const __BUILD_INFO__: BuildInfo;
  /** Injected by `browse-seed-plugin.ts`: the content this build ships, or null. */
  const __CONTENT_SEED__: ContentSeed | null;
  /** Injected by `contact-chat-plugin.ts`: whether this build holds Kettle Chat and the Chat tab. */
  const __KETTLE_CHAT_BUILD__: boolean;
  /**
   * Injected by `contact-chat-plugin.ts` into the chat workers it bundles:
   * true when the dev server builds them, so they may install a test probe
   * (`chat/worker/probe.ts`); false in a build's archives.
   */
  const __WORKER_PROBE__: boolean;
}
