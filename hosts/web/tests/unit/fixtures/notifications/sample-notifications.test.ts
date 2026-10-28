import { describe, expect, test } from "bun:test";
import {
  NotificationStore,
  statusOf,
} from "../../../../src/notifications/notifications.js";
import { MemoryStorage } from "../../../support/test-support.js";
import {
  addSamples,
  clearSamples,
  sampleNotifications,
} from "../../../../fixtures/notifications/sample-notifications.js";

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;

describe("sample notifications", () => {
  test("four waiting, one soon to expire, and three earlier ones", () => {
    const items = sampleNotifications(START);
    const statuses = items.map((item) => statusOf(item, START));
    expect(statuses.filter((status) => status === "pending")).toHaveLength(4);
    expect(statuses.filter((status) => status !== "pending").sort()).toEqual([
      "accepted",
      "declined",
      "expired",
    ]);
    expect(new Set(items.map((item) => item.id)).size).toBe(items.length);
    expect(items.every((item) => item.product.id.endsWith(".example"))).toBe(
      true,
    );
    expect(items.filter((item) => item.seenAt === null)).toHaveLength(4);
    expect(
      Math.min(
        ...items
          .filter((item) => statusOf(item, START) === "pending")
          .map((item) => item.expiresAt),
      ),
    ).toBe(START + 2 * MINUTE);
  });

  test("adding them twice gives one fresh set, and clearing takes only them", () => {
    const store = new NotificationStore(new MemoryStorage(), () => START);
    store.setScope("list");
    store.add({ ...sampleNotifications(START)[0]!, id: "other" });
    addSamples(store);
    store.markSeen(sampleNotifications(START)[0]!.id);
    addSamples(store);
    expect(store.list()).toHaveLength(sampleNotifications(START).length + 1);
    expect(store.unseen()).toBe(5);
    clearSamples(store);
    expect(store.list().map((item) => item.id)).toEqual(["other"]);
  });
});
