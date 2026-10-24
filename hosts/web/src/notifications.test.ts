import { describe, expect, test } from "bun:test";
import { demoNotifications } from "./notifications-demo.js";
import {
  NotificationStore,
  notificationsKey,
  statusOf,
  type HostNotification,
} from "./notifications.js";
import { ago, until } from "./notifications-screen.js";
import { MemoryStorage } from "./test-support.js";

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;

function setup() {
  const clock = { at: START };
  const storage = new MemoryStorage();
  const store = new NotificationStore(storage, () => clock.at);
  const changes: number[] = [];
  store.onChange(() => changes.push(store.unseen()));
  return { clock, storage, store, changes };
}

const signIn = (store: NotificationStore, wallet = "w1", network = "paseo") =>
  store.setScope(notificationsKey(network, wallet));

const one = (overrides: Partial<HostNotification> = {}): HostNotification => ({
  id: "n1",
  origin: "demo",
  kind: "invitation",
  sender: { name: "Ada", handle: "ada.example" },
  product: { id: "chat.example", name: "Example Chat" },
  purpose: "Ada wants to start a chat with you.",
  receivedAt: START - MINUTE,
  expiresAt: START + 10 * MINUTE,
  seenAt: null,
  state: { tag: "pending" },
  ...overrides,
});

describe("host notifications", () => {
  test("seeing an item and answering it are separate steps", () => {
    const { store } = setup();
    signIn(store);
    store.seedDemo([one(), one({ id: "n2" })]);
    expect(store.unseen()).toBe(2);
    expect(statusOf(store.get("n1")!, START)).toBe("pending");

    // Opening one marks only that one seen; it still waits for an answer.
    store.markSeen("n1");
    expect(store.unseen()).toBe(1);
    expect(store.get("n1")!.seenAt).toBe(START);
    expect(statusOf(store.get("n1")!, START)).toBe("pending");
    expect(store.get("n2")!.seenAt).toBeNull();

    expect(store.accept("n1")).toEqual({ ok: true, changed: true });
    expect(store.get("n1")!.state).toEqual({ tag: "accepted", at: START });
    // An answer does not mark an item seen by itself.
    expect(store.decline("n2")).toEqual({ ok: true, changed: true });
    expect(store.get("n2")!.seenAt).toBeNull();
    expect(store.unseen()).toBe(1);
  });

  test("an answer is given once: the same again changes nothing, another is refused", () => {
    const { store, changes } = setup();
    signIn(store);
    store.seedDemo([one()]);
    store.markSeen("n1");
    store.markSeen("n1");
    expect(store.accept("n1")).toEqual({ ok: true, changed: true });
    const after = changes.length;
    expect(store.accept("n1")).toEqual({ ok: true, changed: false });
    expect(store.decline("n1")).toEqual({
      ok: false,
      reason: "This request was already accepted.",
    });
    expect(changes.length).toBe(after);
    expect(store.get("n1")!.state.tag).toBe("accepted");
    expect(store.accept("missing")).toEqual({
      ok: false,
      reason: "This notification is gone.",
    });
  });

  test("a request expires on the clock, cannot be answered after, and the badge still counts it until opened", () => {
    const { clock, store } = setup();
    signIn(store);
    store.seedDemo([
      one(),
      one({ id: "late", expiresAt: START + 60 * MINUTE }),
    ]);
    expect(store.nextExpiry()).toBe(START + 10 * MINUTE);
    clock.at = START + 10 * MINUTE - 1;
    expect(statusOf(store.get("n1")!, clock.at)).toBe("pending");
    clock.at = START + 10 * MINUTE;
    expect(statusOf(store.get("n1")!, clock.at)).toBe("expired");
    expect(store.accept("n1")).toEqual({
      ok: false,
      reason: "This request expired.",
    });
    expect(store.decline("n1")).toEqual({
      ok: false,
      reason: "This request expired.",
    });
    expect(store.get("n1")!.state).toEqual({ tag: "pending" });
    expect(store.nextExpiry()).toBe(START + 60 * MINUTE);
    expect(store.unseen()).toBe(2);
    store.markSeen("n1");
    expect(store.unseen()).toBe(1);
    // An answer given before expiry stands after it.
    store.accept("late");
    clock.at = START + 2 * 60 * MINUTE;
    expect(statusOf(store.get("late")!, clock.at)).toBe("accepted");
    expect(store.nextExpiry()).toBeNull();
  });

  test("pending requests and history survive a reload, and read the same", () => {
    const { clock, storage, store } = setup();
    signIn(store);
    store.seedDemo([one(), one({ id: "n2" }), one({ id: "n3" })]);
    store.markSeen("n1");
    store.accept("n1");
    store.decline("n2");
    clock.at = START + 20 * MINUTE;

    const again = new NotificationStore(storage, () => clock.at);
    signIn(again);
    expect(again.list()).toEqual(store.list());
    expect(again.list().map((item) => statusOf(item, clock.at))).toEqual([
      "accepted",
      "declined",
      "expired",
    ]);
    expect(again.unseen()).toBe(2);
  });

  test("each wallet and network has its own list, and signing out shows none", () => {
    const { storage, store } = setup();
    signIn(store, "w1", "paseo");
    store.seedDemo([one()]);
    signIn(store, "w2", "paseo");
    expect(store.list()).toEqual([]);
    expect(store.unseen()).toBe(0);
    signIn(store, "w1", "previewnet");
    expect(store.list()).toEqual([]);
    store.setScope(null);
    expect(store.scoped()).toBe(false);
    expect(store.list()).toEqual([]);
    // Signed out, nothing is added or written.
    store.seedDemo([one({ id: "ghost" })]);
    expect(store.list()).toEqual([]);
    signIn(store, "w1", "paseo");
    expect(store.list().map((item) => item.id)).toEqual(["n1"]);
    expect(storage.keys()).toEqual([notificationsKey("paseo", "w1")]);
  });

  test("a wallet that never used the demo gets no stored list", () => {
    const { storage, store } = setup();
    signIn(store);
    expect(store.unseen()).toBe(0);
    expect(storage.keys()).toEqual([]);
    store.seedDemo([one()]);
    store.clearDemo();
    expect(store.list()).toEqual([]);
    expect(storage.keys()).toEqual([]);
  });

  test("a damaged list, or an item of another shape, is dropped on load", () => {
    const { storage, store } = setup();
    storage.setItem(notificationsKey("paseo", "w1"), "not json");
    signIn(store);
    expect(store.list()).toEqual([]);
    storage.setItem(
      notificationsKey("paseo", "w2"),
      JSON.stringify([one(), { ...one({ id: "odd" }), origin: "network" }, 3]),
    );
    signIn(store, "w2");
    expect(store.list().map((item) => item.id)).toEqual(["n1"]);
  });

  test("the demo's requests: four waiting, one soon to expire, and three earlier ones", () => {
    const items = demoNotifications(START);
    const statuses = items.map((item) => statusOf(item, START));
    expect(statuses.filter((status) => status === "pending")).toHaveLength(4);
    expect(statuses.filter((status) => status !== "pending").sort()).toEqual([
      "accepted",
      "declined",
      "expired",
    ]);
    expect(new Set(items.map((item) => item.id)).size).toBe(items.length);
    expect(items.every((item) => item.origin === "demo")).toBe(true);
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
    // Seeding twice gives one set, fresh.
    const { store } = setup();
    signIn(store);
    store.seedDemo(items);
    store.markSeen(items[0].id);
    store.seedDemo(items);
    expect(store.list()).toHaveLength(items.length);
    expect(store.unseen()).toBe(4);
  });

  test("times read as a list and a detail show them", () => {
    expect(ago(START, START)).toBe("now");
    expect(ago(START - 4 * MINUTE, START)).toBe("4 min");
    expect(ago(START - 6 * 60 * MINUTE, START)).toBe("6 h");
    expect(ago(START - 3 * 24 * 60 * MINUTE, START)).toBe("3 d");
    expect(until(START + 90_000, START)).toBe("in 2 min");
    expect(until(START + 26 * 60 * MINUTE, START)).toBe("in 1 day");
  });
});
