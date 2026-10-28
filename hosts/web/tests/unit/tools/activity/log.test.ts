import { describe, expect, test } from "bun:test";
import {
  ActivityLog,
  eventText,
  matches,
  type ActivityEvent,
  type ActivityFilter,
} from "../../../../tools/activity/log.js";
import { redactUrl, redactUrls } from "../../../../src/platform/activity.js";

const ANY: ActivityFilter = { category: null, failedOnly: false, query: "" };

function sampleLog(): ActivityLog {
  let now = new Date(2026, 9, 28, 9, 30, 0).getTime();
  const log = new ActivityLog(500, () => (now += 1000));
  log.record({ category: "session", action: "Signed in", subject: "Alice" });
  log.record({
    category: "workers",
    action: "Started",
    subject: "people.paseo",
    details: [{ label: "Record", value: "worker.people.paseo" }],
  });
  log.record({
    category: "products",
    action: "Opened",
    subject: "browse.paseo",
    details: [
      { label: "Product id", value: "browse.paseo" },
      { label: "Execution", value: "App" },
    ],
  });
  log.record({
    category: "products",
    action: "Not opened",
    subject: "pastel.paseo",
    note: "Could not read pastel.paseo",
    failed: true,
  });
  return log;
}

const shown = (log: ActivityLog, filter: Partial<ActivityFilter>) =>
  log
    .list()
    .filter((event) => matches(event, { ...ANY, ...filter }))
    .map((event) => `${event.action} ${event.subject ?? ""}`.trim());

describe("the activity log", () => {
  // Rows are keyed by id, so an id must never be reused, even after a clear.
  test("numbers events in order and never reuses a number", () => {
    const log = sampleLog();
    expect(log.list().map((event) => event.id)).toEqual([1, 2, 3, 4]);
    log.clear();
    log.record({ category: "host", action: "Core ready" });
    expect(log.list().map((event) => event.id)).toEqual([5]);
  });

  test("keeps at most its limit, the oldest dropped first, and counts them", () => {
    const log = new ActivityLog(3);
    for (let index = 1; index <= 5; index++)
      log.record({ category: "host", action: `line ${index}` });
    expect(log.list().map((event) => event.action)).toEqual([
      "line 3",
      "line 4",
      "line 5",
    ]);
    expect(log.dropped()).toBe(2);
    log.clear();
    expect(log.list()).toEqual([]);
    expect(log.dropped()).toBe(0);
  });

  test("tells listeners about each event and a clear", () => {
    const log = new ActivityLog();
    let changes = 0;
    const stop = log.onChange(() => (changes += 1));
    log.record({ category: "host", action: "one" });
    log.clear();
    stop();
    log.record({ category: "host", action: "two" });
    expect(changes).toBe(2);
  });

  test("filters by category, by failure and by text, together", () => {
    const log = sampleLog();
    expect(shown(log, { category: "products" })).toEqual([
      "Opened browse.paseo",
      "Not opened pastel.paseo",
    ]);
    expect(shown(log, { failedOnly: true })).toEqual([
      "Not opened pastel.paseo",
    ]);
    // A search finds what only an event's details say.
    expect(shown(log, { query: "worker.people" })).toEqual([
      "Started people.paseo",
    ]);
    expect(shown(log, { query: "paseo", category: "workers" })).toEqual([
      "Started people.paseo",
    ]);
    expect(shown(log, { query: "session" })).toEqual(["Signed in Alice"]);
    expect(shown(log, { query: "nothing like this" })).toEqual([]);
  });

  test("writes an event as one line and its details below it", () => {
    const [, , opened, failed] = sampleLog().list() as ActivityEvent[];
    expect(eventText(opened)).toBe(
      [
        "09:30:03 Products Opened browse.paseo",
        "  Product id: browse.paseo",
        "  Execution: App",
      ].join("\n"),
    );
    expect(eventText(failed)).toBe(
      "09:30:04 Products FAILED Not opened pastel.paseo (Could not read pastel.paseo)",
    );
  });
});

describe("redaction", () => {
  // A product URL can carry credentials, and the log can be copied anywhere.
  test("drops a URL's user name and password, and secret-looking parameters", () => {
    expect(
      redactUrl(
        "https://user:hunter2@example.test/app?token=abc&page=2&api_key=k#top",
      ),
    ).toBe(
      "https://example.test/app?token=%E2%80%A6&page=2&api_key=%E2%80%A6#top",
    );
  });

  test("leaves the loader's own parameters and text that is no URL", () => {
    const loader =
      "http://localhost:5180/truapi-sandbox/index.html?scope=%2Fproduct%2Fw%2Fa%2F&gateway=https%3A%2F%2Fgw.test&kind=car&start=%2F";
    expect(redactUrl(loader)).toBe(loader);
    expect(redactUrl("browse.paseo")).toBe("browse.paseo");
  });

  test("applies to every text an event holds", () => {
    const log = new ActivityLog();
    log.record({
      category: "products",
      action: "Could not open https://me:pw@app.test/?session=s as app.test",
      subject: "https://me:pw@app.test/",
      note: "wss://rpc.test/?auth=x refused",
      details: [{ label: "URL", value: "https://me:pw@app.test/?secret=y" }],
    });
    const [event] = log.list();
    const text = eventText(event);
    for (const secret of ["me:pw", "session=s", "auth=x", "secret=y"])
      expect(text).not.toContain(secret);
    expect(redactUrls("see https://a.test/?code=1 and more")).toBe(
      "see https://a.test/?code=%E2%80%A6 and more",
    );
  });
});
