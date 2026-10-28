/**
 * Headless-browser test of the inspector's Activity view on its own: the view and
 * an `ActivityLog` mounted on a stand-in page of the host's Vite server, fed
 * synthetic events. No wallet, core, product or chain is involved. It needs
 * a Chromium: playwright-core finds the one it installs, or set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { type Browser, type BrowserContext, type Page } from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
} from "./support/fixtures.js";
import type { Activity } from "../src/platform/activity.js";

const STEP_MS = 60_000;
const TEST_MS = 120_000;
const PAGE = "/__activity-log.html";
/** The stand-in log's limit, small so a test can fill it. */
const LIMIT = 40;

/** What a session's start looks like in the log. */
const SAMPLE: Activity[] = [
  { category: "network", action: "Light client started", subject: "Paseo" },
  { category: "session", action: "Signed in", subject: "Alice" },
  {
    category: "workers",
    action: "Started",
    subject: "people.paseo",
    details: [
      { label: "Record", value: "worker.people.paseo" },
      { label: "Entry", value: "index.js" },
    ],
  },
  {
    category: "products",
    action: "Opened",
    subject: "browse.paseo",
    details: [
      { label: "Product id", value: "browse.paseo" },
      { label: "Execution", value: "App" },
      {
        label: "Loader",
        value:
          "http://localhost:5180/truapi-sandbox/index.html?scope=%2Fproduct%2Fw%2Fbrowse.paseo%2F&kind=car&start=%2F",
      },
    ],
  },
  {
    category: "products",
    action: "Navigate requested",
    subject: "pastel.paseo",
  },
  {
    category: "network",
    action: "Permission denied",
    subject: "pastel.paseo",
    details: [{ label: "Permission", value: "Network access to api.test" }],
  },
  {
    category: "workers",
    action: "Failed",
    subject: "humanity.paseo",
    note: "worker.humanity.paseo draws no humanity card in Pocket.",
    failed: true,
  },
  {
    category: "products",
    action: "Closed",
    subject: "pastel.paseo",
    details: [{ label: "Execution", value: "App" }],
  },
];

const VIEW_PAGE = `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="stylesheet" href="/src/style.css" />
    <style>
      body { display: block; background: #18191d; }
      .devtools { position: static; display: flex; flex-direction: column; width: 420px; height: 560px; padding: 8px; }
    </style>
  </head>
  <body>
    <aside class="devtools">
      <div id="activity-inspector" class="activity-inspector" role="tabpanel" aria-label="Activity">
        <ol id="log"></ol>
      </div>
    </aside>
    <script type="module">
      import { ActivityLog } from "/tools/activity/log.ts";
      import { mountActivityLog } from "/tools/activity/view.ts";
      let now = new Date(2026, 9, 28, 9, 30, 0).getTime();
      const log = new ActivityLog(${LIMIT}, () => (now += 1000));
      mountActivityLog(
        document.getElementById("activity-inspector"),
        document.getElementById("log"),
        log,
      );
      window.__log = {
        record(events) {
          for (const event of events) log.record(event);
        },
        count: () => log.list().length,
      };
    </script>
  </body>
</html>`;

interface LogWindow {
  __log: { record(events: Activity[]): void; count(): number };
}

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function openView(): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    viewport: { width: 480, height: 760 },
  });
  context.setDefaultTimeout(STEP_MS);
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  // Nothing leaves the host's own origin.
  await context.route(
    (url) => !url.href.startsWith(host.origin),
    (route) => route.abort(),
  );
  await context.route(`${host.origin}${PAGE}`, (route) =>
    route.fulfill({ contentType: "text/html", body: VIEW_PAGE }),
  );
  const page = await context.newPage();
  await page.goto(`${host.origin}${PAGE}`);
  await page.waitForFunction(() => "__log" in window);
  return { context, page };
}

const record = (page: Page, events: Activity[]) =>
  page.evaluate(
    (events) => (window as unknown as LogWindow).__log.record(events),
    events,
  );

/** The rows' summaries, as their text reads, in order. */
const rows = (page: Page) =>
  page
    .locator("#log .activity-row > .activity-head")
    .evaluateAll((heads) =>
      heads.map((head) => (head as HTMLElement).innerText.replace(/\s+/g, " ")),
    );

const status = (page: Page) => page.locator(".activity-status").innerText();

/** Wait for the view to draw what was recorded, a microtask later. */
const drawn = (page: Page, count: number) =>
  page.waitForFunction(
    (count) => document.querySelectorAll("#log .activity-row").length === count,
    count,
  );

describe("the activity log", () => {
  test(
    "shows each event as one short row, oldest first, that opens to its details from the keyboard",
    async () => {
      const { context, page } = await openView();
      try {
        expect(await page.locator(".activity-empty").innerText()).toBe(
          "Nothing logged yet.",
        );
        await record(page, SAMPLE);
        await drawn(page, SAMPLE.length);
        expect(await rows(page)).toEqual([
          "09:30:01 Network Light client started Paseo",
          "09:30:02 Session Signed in Alice",
          "09:30:03 Workers Started people.paseo",
          "09:30:04 Products Opened browse.paseo",
          "09:30:05 Products Navigate requested pastel.paseo",
          "09:30:06 Network Permission denied pastel.paseo",
          "09:30:07 Workers Failed humanity.paseo worker.humanity.paseo draws no humanity card in Pocket.",
          "09:30:08 Products Closed pastel.paseo",
        ]);
        expect(await page.locator(".activity-empty").isHidden()).toBe(true);
        // The long loader URL stays out of the row until it is opened.
        const opened = page.locator('#log .activity-row[data-id="4"]');
        const head = opened.locator("button.activity-head");
        expect(await opened.locator(".activity-details").isHidden()).toBe(true);
        await head.focus();
        await page.keyboard.press("Enter");
        expect(await head.getAttribute("aria-expanded")).toBe("true");
        expect(await opened.locator(".activity-details").innerText()).toContain(
          "truapi-sandbox/index.html",
        );

        // New events neither move the open row nor take its focus.
        await record(page, [{ category: "host", action: "Core ready" }]);
        await drawn(page, SAMPLE.length + 1);
        expect(
          await page.evaluate(
            () =>
              (document.activeElement as HTMLElement | null)?.closest("li")
                ?.dataset.id,
          ),
        ).toBe("4");
        expect(await head.getAttribute("aria-expanded")).toBe("true");
        await page.keyboard.press("Space");
        expect(await head.getAttribute("aria-expanded")).toBe("false");
        // A row with nothing more to show is not a button.
        expect(
          await page.locator('#log .activity-row[data-id="2"] button').count(),
        ).toBe(0);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "filters by category and errors, searches details, and says when nothing matches",
    async () => {
      const { context, page } = await openView();
      try {
        await record(page, SAMPLE);
        await drawn(page, SAMPLE.length);
        await page.click('.activity-chip:text-is("Products")');
        expect(await rows(page)).toEqual([
          "09:30:04 Products Opened browse.paseo",
          "09:30:05 Products Navigate requested pastel.paseo",
          "09:30:08 Products Closed pastel.paseo",
        ]);
        expect(
          await page
            .locator('.activity-chip:text-is("Products")')
            .getAttribute("aria-pressed"),
        ).toBe("true");
        // The search reads what only the details say.
        await page.fill(".activity-search", "pastel");
        expect((await rows(page)).length).toBe(2);
        await page.click('.activity-chip:text-is("All")');
        await page.fill(".activity-search", "api.test");
        expect(await rows(page)).toEqual([
          "09:30:06 Network Permission denied pastel.paseo",
        ]);
        await page.fill(".activity-search", "");
        await page.click('.activity-chip:text-is("Errors")');
        expect(await rows(page)).toEqual([
          "09:30:07 Workers Failed humanity.paseo worker.humanity.paseo draws no humanity card in Pocket.",
        ]);
        await page.click('.activity-chip:text-is("Session")');
        expect(await rows(page)).toEqual([]);
        expect(await page.locator(".activity-empty").innerText()).toBe(
          "No events match.",
        );
        expect(await page.locator(".activity-announce").innerText()).toBe(
          "No events match.",
        );
        expect(await status(page)).toContain("0 of 8 events");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "pausing stops following but not recording, and the log still keeps only its limit",
    async () => {
      const { context, page } = await openView();
      try {
        await record(page, SAMPLE);
        await drawn(page, SAMPLE.length);
        await page.click(".activity-follow");
        expect(await page.locator(".activity-follow").innerText()).toBe(
          "Follow",
        );
        await record(page, [
          { category: "host", action: "while paused 1" },
          { category: "host", action: "while paused 2" },
        ]);
        await page.locator(".activity-newer", { hasText: "2 new" }).waitFor();
        expect((await rows(page)).length).toBe(SAMPLE.length);
        expect(
          await page.evaluate(() =>
            (window as unknown as LogWindow).__log.count(),
          ),
        ).toBe(SAMPLE.length + 2);

        // Past the limit, the oldest rows go even while paused.
        await record(
          page,
          Array.from({ length: LIMIT }, (_, index) => ({
            category: "host" as const,
            action: `filler ${index}`,
          })),
        );
        await drawn(page, 0);
        expect(await status(page)).toContain("paused");
        expect(await status(page)).toContain(
          `${SAMPLE.length + 2} older dropped`,
        );

        await page.click(".activity-newer");
        await drawn(page, LIMIT);
        expect(await page.locator(".activity-follow").innerText()).toBe(
          "Pause",
        );
        // Following keeps the newest row in view.
        expect(
          await page
            .locator("#log .activity-row:last-child")
            .evaluate((row) => {
              const list = row.parentElement!.getBoundingClientRect();
              const box = row.getBoundingClientRect();
              return box.bottom <= list.bottom + 1;
            }),
        ).toBe(true);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "copies the rows shown with their details, and clears",
    async () => {
      const { context, page } = await openView();
      try {
        await record(page, SAMPLE);
        await drawn(page, SAMPLE.length);
        await page.click('.activity-chip:text-is("Workers")');
        await page.click('.activity-chip:text-is("Copy")');
        await page.locator('.activity-chip:text-is("Copied")').waitFor();
        expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
          [
            "09:30:03 Workers Started people.paseo",
            "  Record: worker.people.paseo",
            "  Entry: index.js",
            "09:30:07 Workers FAILED Failed humanity.paseo (worker.humanity.paseo draws no humanity card in Pocket.)",
          ].join("\n"),
        );

        await page.click('.activity-chip:text-is("Clear")');
        await drawn(page, 0);
        expect(await page.locator(".activity-empty").innerText()).toBe(
          "Nothing logged yet.",
        );
        expect(await status(page)).toContain("0 of 0 events");
        await record(page, [
          { category: "workers", action: "Started", subject: "x.paseo" },
        ]);
        await drawn(page, 1);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "shows no credentials a URL carried",
    async () => {
      const { context, page } = await openView();
      try {
        await record(page, [
          {
            category: "products",
            action: "Opened",
            subject: "app.test",
            details: [
              {
                label: "URL",
                value: "https://me:hunter2@app.test/?token=abc&page=1",
              },
            ],
          },
        ]);
        await drawn(page, 1);
        await page.click("#log button.activity-head");
        const text = await page.locator("#log").innerText();
        expect(text).toContain("https://app.test/?token=%E2%80%A6&page=1");
        expect(text).not.toContain("hunter2");
        expect(text).not.toContain("abc");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
