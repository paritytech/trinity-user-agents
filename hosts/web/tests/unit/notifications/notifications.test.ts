import { describe, expect, test } from "bun:test";
import {
  NotificationStore,
  notificationsKey,
  statusOf,
  type HostNotification,
} from "../../../src/notifications/notifications.js";
import { ago, until } from "../../../src/notifications/screen.js";
import { MemoryStorage } from "../../support/test-support.js";

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

/** Add each item, as any feature does. */
const addAll = (store: NotificationStore, items: HostNotification[]) => {
  for (const item of items) store.add(item);
};

describe("host notifications", () => {
  test("seeing an item and answering it are separate steps", () => {
    const { store } = setup();
    signIn(store);
    addAll(store, [one(), one({ id: "n2" })]);
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
    addAll(store, [one()]);
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
    addAll(store, [one(), one({ id: "late", expiresAt: START + 60 * MINUTE })]);
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
    addAll(store, [one(), one({ id: "n2" }), one({ id: "n3" })]);
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
    addAll(store, [one()]);
    signIn(store, "w2", "paseo");
    expect(store.list()).toEqual([]);
    expect(store.unseen()).toBe(0);
    signIn(store, "w1", "previewnet");
    expect(store.list()).toEqual([]);
    store.setScope(null);
    expect(store.scoped()).toBe(false);
    expect(store.list()).toEqual([]);
    // Signed out, nothing is added or written.
    expect(store.add(one({ id: "ghost" }))).toBeNull();
    expect(store.list()).toEqual([]);
    signIn(store, "w1", "paseo");
    expect(store.list().map((item) => item.id)).toEqual(["n1"]);
    expect(storage.keys()).toEqual([notificationsKey("paseo", "w1")]);
  });

  test("a list is stored only while it holds something", () => {
    const { storage, store } = setup();
    signIn(store);
    expect(store.unseen()).toBe(0);
    expect(storage.keys()).toEqual([]);
    addAll(store, [one(), one({ id: "n2" })]);
    store.remove(["n1", "n2", "unknown"]);
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
      JSON.stringify([
        one(),
        { ...one({ id: "odd" }), state: { tag: "maybe" } },
        3,
      ]),
    );
    signIn(store, "w2");
    expect(store.list().map((item) => item.id)).toEqual(["n1"]);
  });

  test("an item is added once; a repeat keeps the first, and each answer that changes it is reported once", () => {
    const { store } = setup();
    signIn(store);
    const answers: string[] = [];
    store.onAnswer((item) => answers.push(`${item.id}:${item.state.tag}`));
    expect(store.add(one())?.added).toBe(true);
    store.markSeen("n1");
    const again = store.add(one({ purpose: "changed" }));
    expect(again?.added).toBe(false);
    expect(again?.item.purpose).toBe(one().purpose);
    expect(again?.item.seenAt).toBe(START);
    store.accept("n1");
    store.accept("n1");
    store.decline("n1");
    expect(answers).toEqual(["n1:accepted"]);
    // Removing one leaves the others.
    store.add(one({ id: "n2" }));
    store.remove(["n2"]);
    expect(store.list().map((item) => item.id)).toEqual(["n1"]);
  });

  test("a list saved by an earlier version loads, and keeps the fields this one does not read", () => {
    const { storage, store } = setup();
    const key = notificationsKey("paseo", "w1");
    storage.setItem(
      key,
      JSON.stringify([
        { ...one(), origin: "demo" },
        {
          ...one({ id: "n2" }),
          origin: "local-delivery",
          delivery: { from: "ada", network: "paseo", payload: "0xab" },
        },
      ]),
    );
    signIn(store);
    expect(store.list().map((item) => item.id)).toEqual(["n1", "n2"]);
    store.accept("n1");
    const saved = JSON.parse(storage.getItem(key)!) as Record<
      string,
      unknown
    >[];
    expect(saved.map((item) => item.origin)).toEqual([
      "demo",
      "local-delivery",
    ]);
    expect(saved[1]!.delivery).toEqual({
      from: "ada",
      network: "paseo",
      payload: "0xab",
    });
  });

  test("dismissing takes an item off the list only: no answer, its state kept, and it still expires", () => {
    const { clock, storage, store } = setup();
    signIn(store);
    const answers: string[] = [];
    store.onAnswer((item) => answers.push(item.id));
    addAll(store, [one(), one({ id: "n2" })]);
    store.dismiss("n1");
    store.dismiss("n1");
    expect(answers).toEqual([]);
    expect(store.listed().map((item) => item.id)).toEqual(["n2"]);
    expect(store.list().map((item) => item.id)).toEqual(["n1", "n2"]);
    // A dismissed item is not counted as unseen, and is still pending.
    expect(store.unseen()).toBe(1);
    expect(store.get("n1")!.state).toEqual({ tag: "pending" });
    expect(store.get("n1")!.dismissedAt).toBe(START);
    clock.at = START + 10 * MINUTE;
    expect(statusOf(store.get("n1")!, clock.at)).toBe("expired");
    // It survives a reload as dismissed, and a repeat delivery does not bring it back.
    const again = new NotificationStore(storage, () => clock.at);
    signIn(again);
    expect(again.listed().map((item) => item.id)).toEqual(["n2"]);
    expect(again.add(one())?.added).toBe(false);
    expect(again.listed().map((item) => item.id)).toEqual(["n2"]);
  });

  test("several items are marked seen at once, each only the first time", () => {
    const { clock, store, changes } = setup();
    signIn(store);
    addAll(store, [one(), one({ id: "n2" }), one({ id: "n3" })]);
    const before = changes.length;
    store.markSeen("n1", "n2");
    expect(changes.length).toBe(before + 1);
    clock.at = START + MINUTE;
    store.markSeen("n1", "n2", "missing");
    expect(changes.length).toBe(before + 1);
    expect(store.get("n1")!.seenAt).toBe(START);
    expect(store.unseen()).toBe(1);
  });

  // The Apps shelf badges each product from the same items as the Inbox, so
  // the per-product counts add up to the Inbox count and clear with it.
  test("unseen counts per product come from the same items as the total", () => {
    const { store } = setup();
    signIn(store);
    const other = { id: "notes.example", name: "Example Notes" };
    addAll(store, [
      one(),
      one({ id: "n2" }),
      one({ id: "n3", product: other }),
      one({ id: "n4", product: other, dismissedAt: START }),
    ]);
    const counts = () => [
      store.unseen(),
      store.unseen("chat.example"),
      store.unseen(other.id),
      store.unseen("missing.example"),
    ];
    expect(counts()).toEqual([3, 2, 1, 0]);
    store.markSeen("n1", "n3");
    expect(counts()).toEqual([1, 1, 0, 0]);
  });

  test("times read as a list shows them", () => {
    expect(ago(START, START)).toBe("now");
    expect(ago(START - 4 * MINUTE, START)).toBe("4 min");
    expect(ago(START - 6 * 60 * MINUTE, START)).toBe("6 h");
    expect(ago(START - 3 * 24 * 60 * MINUTE, START)).toBe("3 d");
    expect(until(START + 90_000, START)).toBe("in 2 min");
    expect(until(START + 26 * 60 * MINUTE, START)).toBe("in 1 day");
  });
});
