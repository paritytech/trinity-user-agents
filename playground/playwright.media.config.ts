import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/media",
  workers: 1,
  timeout: 30_000,
  use: {
    browserName: "chromium",
    headless: true,
    viewport: { width: 800, height: 600 },
    launchOptions: {
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
      args: [
        "--use-fake-device-for-media-stream",
        "--use-fake-ui-for-media-stream",
      ],
    },
  },
});
