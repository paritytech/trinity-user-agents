// Runs one host-playground test inside the page and reports how it ended.
//
// Injected by both drivers: over CDP on Android, through the WKWebView on iOS.
// It defines `window.__hostPlaygroundE2E.runOne(id, timeoutMs)`, which clicks
// the test's run button and waits for the log entry the click adds to settle.
// The driver calls it once per test, so a test that navigates away loses only
// itself, and a native sheet the test raises can be answered from outside
// while the promise is pending.
//
// Clicks go through `element.click()` rather than a pointer event, because on
// a phone-width viewport the playground covers the page with its log sheet as
// soon as a test starts.
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

  const entries = () => document.querySelectorAll('[data-testid="log-entry"]');
  // Entries are prepended, so the newest is first. It is read again on every
  // poll rather than held, since a re-render can replace the element.
  const newest = () => entries()[0] ?? null;

  async function runOne(id, timeoutMs = 60000) {
    const started = Date.now();
    const result = (fields) => ({ id, durationMs: Date.now() - started, ...fields });

    const button = await waitFor(
      () => document.querySelector(`[data-testid="run-${id}"]`),
      15000,
    );
    if (!button) return result({ status: "missing", message: "no run button in the page" });

    // A button is disabled while another test runs, while its argument
    // defaults resolve, and for good when the test only runs from a worker.
    const enabled = await waitFor(() => !button.disabled, 15000);
    if (!enabled) return result({ status: "skipped", message: "run button stayed disabled" });

    const before = entries().length;
    button.scrollIntoView({ block: "center" });
    button.click();

    const appeared = await waitFor(() => entries().length > before, 10000);
    if (!appeared) return result({ status: "error", message: "the click added no log entry" });

    const settled = await waitFor(() => {
      const entry = newest();
      return entry && entry.dataset.status !== "pending" ? entry : null;
    }, timeoutMs, 250);
    if (!settled) return result({ status: "timeout", message: `no result within ${timeoutMs} ms` });

    return result({
      status: settled.dataset.status,
      outcome: settled.dataset.outcome || undefined,
      message: settled.querySelector("div.break-all")?.textContent?.trim() || undefined,
      // The entry's JSON, which carries what the message abbreviates, such as full addresses.
      detail: settled.querySelector("pre")?.textContent?.trim().slice(0, 4000) || undefined,
    });
  }

  // Whether the playground has finished deciding it is inside a host and has
  // rendered its buttons.
  const ready = () => document.querySelector('[data-testid^="run-"]') !== null;

  window.__hostPlaygroundE2E = { runOne, ready };
})();
