import { describe, expect, test } from "bun:test";
import type { ProductContext } from "@parity/truapi-host";
import { createGameReminders } from "../../../src/app/callbacks.js";
import type { Activity } from "../../../src/platform/activity.js";

const product: ProductContext = {
  productId: "game.example",
  executionKind: "Worker",
};

function setup() {
  const log: Activity[] = [];
  return { log, game: createGameReminders((entry) => log.push(entry)) };
}

const actions = (log: Activity[]) => log.map((entry) => entry.action);

describe("createGameReminders", () => {
  // A host without this answers the game product `Unsupported`, which its
  // worker reads as an outdated app and holds back Register.
  test("holds a reminder and announces it once when the game starts", async () => {
    const { log, game } = setup();
    await game.scheduleGameReminder(product, BigInt(Date.now() + 20));
    expect(actions(log)).toEqual(["Game reminder set"]);
    await Bun.sleep(80);
    expect(actions(log)).toEqual(["Game reminder set", "Game starting"]);
    await game.cancelGameReminder(product);
    expect(actions(log)).toEqual(["Game reminder set", "Game starting"]);
  });

  test("keeps one reminder per product, so a new start replaces the old one", async () => {
    const { log, game } = setup();
    await game.scheduleGameReminder(product, BigInt(Date.now() + 20));
    await game.scheduleGameReminder(product, BigInt(Date.now() + 60_000));
    await Bun.sleep(80);
    expect(actions(log)).toEqual(["Game reminder set", "Game reminder set"]);
    await game.cancelGameReminder(product);
  });

  test("cancels a held reminder, and cancelling none succeeds quietly", async () => {
    const { log, game } = setup();
    await game.cancelGameReminder(product);
    expect(log).toEqual([]);
    await game.scheduleGameReminder(product, BigInt(Date.now() + 20));
    await game.cancelGameReminder(product);
    await Bun.sleep(80);
    expect(actions(log)).toEqual([
      "Game reminder set",
      "Game reminder cancelled",
    ]);
  });
});
