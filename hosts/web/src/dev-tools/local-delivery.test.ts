import { describe, expect, test } from "bun:test";
import type { IdentityKeys } from "../invitations/ports.js";
import { cryptoRandom } from "../invitations/sealing.js";
import {
  LocalDelivery,
  type ReturnRoutes,
  POST_LIFETIME_MS,
  REPOST_INTERVAL_MS,
  deliveryPrefix,
} from "./local-delivery.js";
import { MemoryWorld } from "./shared-store.js";

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const HOUR = 60 * 60_000;
const SEALED = `0x${"ab".repeat(64)}`;
const ADA: IdentityKeys = {
  signingPublic: `0x${"01".repeat(32)}`,
  agreementPublic: `0x${"0a".repeat(32)}`,
};
const KIM: IdentityKeys = {
  signingPublic: `0x${"03".repeat(32)}`,
  agreementPublic: `0x${"0c".repeat(32)}`,
};
const LIN: IdentityKeys = {
  signingPublic: `0x${"02".repeat(32)}`,
  agreementPublic: `0x${"0b".repeat(32)}`,
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
  const host = (
    self: IdentityKeys,
    network = "testnet",
    returnRoutes: ReturnRoutes = "inbox",
  ) => {
    const requests: string[] = [];
    const replies: string[] = [];
    const delivery = new LocalDelivery({
      self: self.agreementPublic,
      network,
      store: world.store(),
      now: clock.now,
      schedule: clock.schedule,
      random: cryptoRandom,
      returnRoutes,
      log: () => {},
    });
    delivery.listen({
      request: (sealed) => requests.push(sealed),
      reply: (route, sealed) => replies.push(`${route} ${sealed}`),
    });
    return { delivery, requests, replies };
  };
  return { world, clock, host };
}

describe("local delivery", () => {
  test("delivers a request to its recipient only", () => {
    const { world, host } = pair();
    const ada = host(ADA);
    const lin = host(LIN);
    ada.delivery.postRequest(LIN, "one", SEALED, START + HOUR);
    expect(lin.requests).toEqual([SEALED]);
    expect(ada.requests).toEqual([]);
    expect(world.items.size).toBe(1);
  });

  for (const kind of ["inbox", "topic"] as const)
    test(`with ${kind} return routes, hands a reply only to the host whose route it is, while it watches it`, () => {
      const { host } = pair();
      const ada = host(ADA, "testnet", kind);
      const lin = host(LIN, "testnet", kind);
      const kim = host(KIM, "testnet", kind);
      const route = ada.delivery.returnRoute();
      const other = ada.delivery.returnRoute();
      expect(route.startsWith(`${kind}:`)).toBe(true);
      expect(route).not.toBe(other);
      // Reachable for Ada, who made it; an inbox route reaches no one else.
      expect(lin.delivery.isReturnRoute(route, ADA)).toBe(true);
      expect(lin.delivery.isReturnRoute(route, KIM)).toBe(kind === "topic");
      expect(lin.delivery.isReturnRoute(`elsewhere:${route}`, ADA)).toBe(false);

      lin.delivery.postReply(route, "reply", SEALED, START + HOUR);
      expect(ada.replies).toEqual([]);
      ada.delivery.watch(route);
      expect(ada.replies).toEqual([`${route} ${SEALED}`]);
      // A reply on another route of Ada's is not handed over while she does not watch it.
      lin.delivery.postReply(
        other,
        "other",
        `0x${"ef".repeat(64)}`,
        START + HOUR,
      );
      expect(ada.replies).toHaveLength(1);
      ada.delivery.unwatch(route);
      lin.delivery.postReply(
        route,
        "again",
        `0x${"cd".repeat(64)}`,
        START + HOUR,
      );
      expect(ada.replies).toHaveLength(1);
      // Replies are never taken as requests, and no one else hears them.
      expect([ada.requests, lin.requests, kim.requests, kim.replies]).toEqual([
        [],
        [],
        [],
        [],
      ]);
    });

  test("posts again on its cadence until withdrawn, and reaches a host that starts later", () => {
    const { world, clock, host } = pair();
    const ada = host(ADA);
    ada.delivery.postRequest(LIN, "one", SEALED, START + HOUR);
    clock.advance(REPOST_INTERVAL_MS);
    const lin = host(LIN);
    expect(lin.requests).toEqual([SEALED]);
    clock.advance(REPOST_INTERVAL_MS);
    expect(lin.requests).toEqual([SEALED, SEALED]);
    ada.delivery.withdraw("one");
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(lin.requests).toHaveLength(2);
    // A withdrawn post lapses on its own.
    clock.advance(POST_LIFETIME_MS);
    expect(world.items.size).toBe(0);
    expect(host(LIN).requests).toEqual([]);
  });

  test("stops at `until`, and a lapsed post is not delivered", () => {
    const { clock, host } = pair();
    const ada = host(ADA);
    ada.delivery.postRequest(
      LIN,
      "one",
      SEALED,
      START + 2 * REPOST_INTERVAL_MS,
    );
    clock.advance(2 * REPOST_INTERVAL_MS);
    expect(host(LIN).requests).toEqual([]);
  });

  test("keeps networks apart, and close stops posting and hearing", () => {
    const { world, clock, host } = pair();
    const ada = host(ADA);
    const other = host(LIN, "othernet");
    ada.delivery.postRequest(LIN, "one", SEALED, START + HOUR);
    expect(other.requests).toEqual([]);
    const lin = host(LIN);
    lin.delivery.close();
    ada.delivery.close();
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(lin.requests).toEqual([SEALED]);
    expect(
      [...world.items.keys()].every((key) =>
        key.startsWith(deliveryPrefix("testnet")),
      ),
    ).toBe(true);
  });
});
