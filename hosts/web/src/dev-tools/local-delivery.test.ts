import { describe, expect, test } from "bun:test";
import type { PeerKeys } from "../invitations/binding.js";
import {
  LocalDelivery,
  POST_LIFETIME_MS,
  REPOST_INTERVAL_MS,
  mailboxPrefix,
} from "./local-delivery.js";
import { MemoryWorld } from "./shared-store.js";
import { StatementFixture } from "./statement-fixture.js";

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const HOUR = 60 * 60_000;
const SEALED = `0x${"ab".repeat(64)}`;
const ADA: PeerKeys = {
  signingPublic: "0x01",
  inboxPublic: `0x${"0a".repeat(32)}`,
};
const LIN: PeerKeys = {
  signingPublic: "0x02",
  inboxPublic: `0x${"0b".repeat(32)}`,
};

class Clock {
  at = START;
  private tasks = new Map<number, { due: number; task: () => void }>();
  private next = 0;
  readonly now = (): number => this.at;
  readonly schedule = (task: () => void, delayMs: number): (() => void) => {
    const id = this.next++;
    this.tasks.set(id, { due: this.at + delayMs, task });
    return () => this.tasks.delete(id);
  };
  advance(ms: number): void {
    const end = this.at + ms;
    for (;;) {
      const due = [...this.tasks.entries()]
        .filter(([, entry]) => entry.due <= end)
        .sort(([, left], [, right]) => left.due - right.due)[0];
      if (due === undefined) break;
      this.tasks.delete(due[0]);
      this.at = due[1].due;
      due[1].task();
    }
    this.at = end;
  }
}

function pair() {
  const world = new MemoryWorld();
  const clock = new Clock();
  const host = (self: PeerKeys, network = "testnet") => {
    const heard: string[] = [];
    const delivery = new LocalDelivery({
      self: self.inboxPublic,
      network,
      store: world.store(),
      now: clock.now,
      schedule: clock.schedule,
      log: () => {},
    });
    delivery.listen((sealed) => heard.push(sealed));
    return { delivery, heard };
  };
  return { world, clock, host };
}

describe("local delivery", () => {
  test("delivers to the recipient only, and the recipient writes nothing back", () => {
    const { world, host } = pair();
    const ada = host(ADA);
    const lin = host(LIN);
    ada.delivery.post(LIN, "one", SEALED, START + HOUR);
    expect(lin.heard).toEqual([SEALED]);
    expect(ada.heard).toEqual([]);
    expect(world.items.size).toBe(1);
  });

  test("posts again on its cadence until withdrawn, and reaches a host that starts later", () => {
    const { world, clock, host } = pair();
    const ada = host(ADA);
    ada.delivery.post(LIN, "one", SEALED, START + HOUR);
    clock.advance(REPOST_INTERVAL_MS);
    const lin = host(LIN);
    expect(lin.heard).toEqual([SEALED]);
    clock.advance(REPOST_INTERVAL_MS);
    expect(lin.heard).toEqual([SEALED, SEALED]);
    ada.delivery.withdraw("one");
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(lin.heard).toHaveLength(2);
    // A withdrawn post lapses on its own.
    clock.advance(POST_LIFETIME_MS);
    expect(world.items.size).toBe(0);
    expect(host(LIN).heard).toEqual([]);
  });

  test("stops at `until`, and a lapsed post is not delivered", () => {
    const { clock, host } = pair();
    const ada = host(ADA);
    ada.delivery.post(LIN, "one", SEALED, START + 2 * REPOST_INTERVAL_MS);
    clock.advance(2 * REPOST_INTERVAL_MS);
    expect(host(LIN).heard).toEqual([]);
  });

  test("keeps networks apart, and close stops posting and hearing", () => {
    const { world, clock, host } = pair();
    const ada = host(ADA);
    const other = host(LIN, "othernet");
    ada.delivery.post(LIN, "one", SEALED, START + HOUR);
    expect(other.heard).toEqual([]);
    const lin = host(LIN);
    lin.delivery.close();
    ada.delivery.close();
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(lin.heard).toEqual([SEALED]);
    expect(
      [...world.items.keys()].every((key) =>
        key.startsWith(mailboxPrefix("testnet")),
      ),
    ).toBe(true);
  });
});

describe("the statement store stand-in", () => {
  test("hands a product what is posted on the topics it watches, from any tab, including what was posted before", () => {
    const world = new MemoryWorld();
    const clock = new Clock();
    const topic = `0x${"7a".repeat(32)}`;
    const fixture = (heard: string[]) =>
      new StatementFixture({
        network: "testnet",
        store: world.store(),
        now: clock.now,
        send: (message) => heard.push(message.data),
      });
    const aliceHeard: string[] = [];
    const bobHeard: string[] = [];
    const alice = fixture(aliceHeard);
    const bob = fixture(bobHeard);
    alice.call({ statements: "subscribe", topic });
    bob.call({ statements: "post", topic, data: "0x01" });
    bob.call({
      statements: "post",
      topic: `0x${"7b".repeat(32)}`,
      data: "0x02",
    });
    expect(aliceHeard).toEqual(["0x01"]);
    const late: string[] = [];
    fixture(late).call({ statements: "subscribe", topic });
    expect(late).toEqual(["0x01"]);
    expect(bobHeard).toEqual([]);
  });
});
