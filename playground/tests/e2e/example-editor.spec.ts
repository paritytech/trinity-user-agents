import { expect, test } from "@playwright/test";
import { openPlayground, selectMethod, waitForOnline } from "./helpers";

test.describe("example editor", () => {
  // Native hosts' lockdown container removes `Worker`, so the editor must not
  // depend on one.
  test("edits an example without Web Workers", async ({ page }) => {
    await page.addInitScript(() => Reflect.deleteProperty(window, "Worker"));
    const pageErrors: string[] = [];
    page.on("pageerror", (error) => pageErrors.push(error.message));

    const frame = await openPlayground(page);
    await waitForOnline(frame);
    await selectMethod(frame, "System", "feature_supported");

    const editor = frame.locator(".monaco-editor .view-lines");
    await editor.click();
    await page.keyboard.press("Control+End");
    await page.keyboard.type("\nconst edited = true;");
    await expect(editor).toContainText("const edited = true;");
    await page.waitForTimeout(1_000);

    expect(pageErrors).toEqual([]);
  });
});
