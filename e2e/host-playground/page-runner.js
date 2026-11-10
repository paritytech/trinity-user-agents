// Injected by both drivers. `runOne(id, timeoutMs)` clicks a test's run button and waits for its
// log entry to settle. Clicks use `element.click()` because the log sheet covers the page on a phone.
(() => {
  if (window.__hostPlaygroundE2E) return;

  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  async function waitFor(read, timeoutMs, stepMs = 100) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const value = read();
      if (value) return value;
      if (Date.now() >= deadline) return null;
      await sleep(stepMs);
    }
  }

  const BUTTON_WAIT_MS = 15_000;
  const LOG_ENTRY_WAIT_MS = 10_000;
  const SETTLE_POLL_MS = 250;
  const DETAIL_LIMIT = 4_000;

  const entries = () => document.querySelectorAll('[data-testid="log-entry"]');
  // Re-read on every poll, since a re-render can replace the element.
  const newest = () => entries()[0] ?? null;

  // The navigation can land before or after the log entry settles, so these pass on reaching the path.
  const IN_APP_DESTINATIONS = { "navigate-internal": "/navigation" };

  // Their detail holds key material, and results are published as artifacts.
  const SECRET_DETAIL = new Set(["derive-entropy"]);

  async function runOne(id, timeoutMs) {
    const started = Date.now();
    const result = (fields) => ({ id, durationMs: Date.now() - started, ...fields });

    const button = await waitFor(
      () => document.querySelector(`[data-testid="run-${id}"]`),
      BUTTON_WAIT_MS,
    );
    if (!button) return result({ status: "missing", message: "no run button in the page" });

    // Disabled for good when the test only runs from a worker.
    const enabled = await waitFor(() => !button.disabled, BUTTON_WAIT_MS);
    if (!enabled) return result({ status: "skipped", message: "run button stayed disabled" });

    const destination = IN_APP_DESTINATIONS[id];
    const arrived = () => destination !== undefined && location.pathname.startsWith(destination);
    const navigated = () =>
      result({ status: "success", outcome: "navigated", message: `${location.pathname}${location.search}${location.hash} opened` });

    const before = entries().length;
    button.scrollIntoView({ block: "center" });
    button.click();

    const appeared = await waitFor(() => arrived() || entries().length > before, LOG_ENTRY_WAIT_MS);
    if (arrived()) return navigated();
    if (!appeared) return result({ status: "error", message: "the click added no log entry" });

    const settled = await waitFor(() => {
      if (arrived()) return true;
      const entry = newest();
      return entry && entry.dataset.status !== "pending" ? entry : null;
    }, timeoutMs, SETTLE_POLL_MS);
    if (arrived()) return navigated();
    if (!settled) return result({ status: "timeout", message: `no result within ${timeoutMs} ms` });
    if (destination !== undefined && settled.dataset.status === "success") {
      if (await waitFor(arrived, timeoutMs)) return navigated();
      return result({ status: "error", message: `${destination} never opened` });
    }

    return result({
      status: settled.dataset.status,
      outcome: settled.dataset.outcome || undefined,
      message: settled.querySelector("div.break-all")?.textContent?.trim() || undefined,
      detail: SECRET_DETAIL.has(id) ? undefined : settled.querySelector("pre")?.textContent?.trim().slice(0, DETAIL_LIMIT) || undefined,
    });
  }

  // Sends a page without tests back to the list.
  const ready = () => {
    if (document.querySelector('[data-testid^="run-"]') !== null) return true;
    if (location.pathname !== "/") location.assign("/");
    return false;
  };

  window.__hostPlaygroundE2E = { runOne, ready };
})();
