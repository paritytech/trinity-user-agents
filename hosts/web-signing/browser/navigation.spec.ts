/**
 * Headless-browser check of what the host does when a connected product
 * navigates inside its frame. A link, a form post or a script loads a new
 * document, and the container in that document is not answered: the host says
 * so and gives the page no channel until the product is opened again. A
 * single-page navigation loads no document and must change nothing.
 *
 * The product is the fixture page that loads the container itself, so every
 * path on it is a new container. Requests are judged by whether they reached
 * the counting server. The environment is the smoke test's.
 */
import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  test,
} from "bun:test";
import {
  type Browser,
  type BrowserContext,
  type Frame,
  type Page,
  chromium,
} from "playwright-core";
import {
  type CountingTarget,
  type RunningHost,
  type UrlProduct,
  TEST_MNEMONIC,
  startCountingTarget,
  startHost,
  startUrlProduct,
} from "./fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 180_000;

interface ProductWindow {
  __ask(path: string): Promise<"sent" | "refused">;
}

let browser: Browser;
let host: RunningHost;
let target: CountingTarget;
let urlProduct: UrlProduct;

beforeAll(async () => {
  host = await startHost();
  target = await startCountingTarget();
  urlProduct = await startUrlProduct(target.origin);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || undefined,
  });
}, STEP_MS);

afterAll(async () => {
  await browser?.close();
  urlProduct?.close();
  target?.close();
  await host?.close();
});

beforeEach(() => {
  target.hits.length = 0;
});

async function signedInPage(context: BrowserContext): Promise<Page> {
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor();
  if ((await page.getAttribute("body", "data-menu")) !== "open")
    await page.click("#menu-toggle");
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", TEST_MNEMONIC);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
  return page;
}

async function productFrame(page: Page): Promise<Frame> {
  const origin = new URL(urlProduct.url).origin;
  const deadline = Date.now() + STEP_MS;
  while (Date.now() < deadline) {
    const frame = page
      .frames()
      .find((candidate) => candidate.url().startsWith(origin));
    const ready = await frame
      ?.evaluate(() => "__ask" in window)
      .catch(() => false);
    if (frame && ready) return frame;
    await page.waitForTimeout(200);
  }
  throw new Error(`no product page appeared at ${origin}`);
}

async function openContainerProduct(page: Page): Promise<Frame> {
  await page.check("#expect-container");
  await page.fill("#address", urlProduct.url);
  await page.press("#address", "Enter");
  return productFrame(page);
}

const status = (page: Page) =>
  page.evaluate(() => document.getElementById("product-status")?.textContent);

async function waitForStatus(page: Page, text: string): Promise<void> {
  await page.waitForFunction(
    (wanted) =>
      document.getElementById("product-status")?.textContent?.includes(wanted),
    text,
  );
}

/** Answer the open prompt. */
async function answer(
  page: Page,
  choice: "Deny" | "Allow once" | "Always allow",
): Promise<void> {
  const dialog = page.locator("dialog.prompt");
  await dialog.waitFor();
  await dialog.getByRole("button", { name: choice, exact: true }).click();
  await dialog.waitFor({ state: "detached" });
}

/** Ask for `path` from the page and answer the prompt the request raises. */
async function askAndAllow(page: Page, frame: Frame, path: string) {
  const result = frame.evaluate(
    (requested) => (window as unknown as ProductWindow).__ask(requested),
    path,
  );
  await answer(page, "Always allow");
  return result;
}

describe("a connected product that navigates inside its frame", () => {
  test(
    "a new document ends the connection, is shown as ended, and is not given a channel until reopened",
    async () => {
      const context = await browser.newContext();
      context.setDefaultTimeout(STEP_MS);
      try {
        const page = await signedInPage(context);
        const frame = await openContainerProduct(page);
        await answer(page, "Allow once");
        await waitForStatus(page, "Container: on");
        expect(target.hits).toEqual(["/first"]);

        await frame.evaluate(() => location.assign("/elsewhere"));
        await waitForStatus(page, "Container: ended");
        expect(await page.textContent("#open")).toBe("Reopen");
        expect(await status(page)).not.toContain("Container: on");

        const next = await productFrame(page);
        expect(new URL(next.url()).pathname).toBe("/elsewhere");
        const asked = next.evaluate(() =>
          (window as unknown as ProductWindow).__ask("/after"),
        );
        await page.waitForTimeout(1500);
        expect(await page.locator("dialog.prompt").count()).toBe(0);
        expect(target.hits).toEqual(["/first"]);
        void asked.catch(() => undefined);

        await page.click("#open");
        const reopened = await productFrame(page);
        await waitForStatus(page, "Container: on");
        expect(await page.textContent("#open")).toBe("Open");
        expect(new URL(reopened.url()).pathname).toBe("/");
        expect(await askAndAllow(page, reopened, "/reopened")).toBe("sent");
        expect(target.hits).toContain("/reopened");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a single-page navigation keeps the connection",
    async () => {
      const context = await browser.newContext();
      context.setDefaultTimeout(STEP_MS);
      try {
        const page = await signedInPage(context);
        const frame = await openContainerProduct(page);
        await answer(page, "Allow once");
        await waitForStatus(page, "Container: on");

        await frame.evaluate(() => history.pushState({}, "", "/spa/route"));
        await page.waitForTimeout(1500);

        expect(await status(page)).toContain("Container: on");
        expect(await page.textContent("#open")).toBe("Open");
        expect(await askAndAllow(page, frame, "/still-connected")).toBe("sent");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
