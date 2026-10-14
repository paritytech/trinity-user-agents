/**
 * Menus, participants and reaction details of the sample product on the test
 * host: S25, S26, S28, S30 and S32 of 04-TESTING §4.
 */
import { afterEach, describe, expect, test } from "bun:test";
import type { ChatMessage } from "@parity/truapi";
import { ACTION_MENTION, ACTION_PIN, ACTION_REPORT } from "../../worker/fixtures";
import { startOnTestHost, testHostIsBuilt, until, waitForText, type Running } from "../support/test-host";

const suite = testHostIsBuilt() ? describe : describe.skip;

let running: Running | undefined;
async function start() {
  running = await startOnTestHost();
  await running.host.chatUser.open("source");
  return running;
}
afterEach(() => {
  running?.dispose();
  running = undefined;
});

async function command(run: Running, line: string): Promise<void> {
  await run.host.chatUser.send("source", line);
  await waitForText(run.host, "source", `ok ${line}`);
}

/** A message of the user in `source`, so the host menu has Edit and Delete. */
async function ownMessage(run: Running): Promise<ChatMessage> {
  const outcome = await run.host.chatUser.send("source", "mine");
  if (outcome.tag !== "Posted") throw new Error(`send gave ${outcome.tag}`);
  return outcome.message;
}

suite("menus, participants and reactions on the test host", () => {
  test("S25: ExtendBuiltIn shows the host entries without hidden, then the product entries", async () => {
    const run = await start();
    const mine = await ownMessage(run);
    const menu = () => run.host.chatUser.messageMenu("source", mine.messageId);

    // `/menu none` turns the declaration off: the host menu, with no query.
    await command(run, "/menu none");
    expect(run.host.room("source")?.config.messageActions).toBe(false);
    expect(await menu()).toEqual({
      builtIn: ["Copy", "Reply", "Edit", "Delete", "React"],
      actions: [],
      source: "default",
    });
    expect(run.host.chatUser.queries().some(({ name }) => name === "chatMessageActionsQuery")).toBe(false);

    await command(run, "/menu hide-edit");
    expect(await menu()).toEqual({
      builtIn: ["Copy", "Reply", "Delete", "React"],
      actions: [ACTION_PIN],
      source: "product",
    });
    await command(run, "/menu extend");
    expect(await menu()).toEqual({
      builtIn: ["Copy", "Reply", "Edit", "Delete", "React"],
      actions: [ACTION_PIN],
      source: "product",
    });
    // The query carries the message, as the host shows it.
    const asked = run.host.chatUser.queries().filter(({ name }) => name === "chatMessageActionsQuery");
    expect(asked.at(-1)?.request).toMatchObject({ roomId: "source", message: { messageId: mine.messageId } });
  });

  test("S26: Custom shows only the product entries, and a time-out shows the host menu", async () => {
    const run = await start();
    const mine = await ownMessage(run);
    await command(run, "/menu custom");
    expect(await run.host.chatUser.messageMenu("source", mine.messageId)).toEqual({
      builtIn: [],
      actions: [ACTION_PIN, ACTION_REPORT],
      source: "product",
    });

    await command(run, "/slow 400");
    run.host.chatUser.setTimings({ details: 100 });
    expect(await run.host.chatUser.messageMenu("source", mine.messageId)).toEqual({
      builtIn: ["Copy", "Reply", "Edit", "Delete", "React"],
      actions: [],
      source: "default",
    });
    expect(run.host.chatUser.queries().at(-1)?.result._unsafeUnwrapErr()).toEqual({ tag: "Timeout" });
  });

  test("S28: a callback entry gives MessageActionTriggered", async () => {
    const run = await start();
    const mine = await ownMessage(run);
    await command(run, "/menu custom");
    const menu = await run.host.chatUser.messageMenu("source", mine.messageId);

    await run.host.chatUser.pickMessageAction("source", mine.messageId, menu.actions[0]!);
    expect(run.host.events().at(-1)?.event).toEqual({
      tag: "MessageActionTriggered",
      value: { messageId: mine.messageId, actionId: "act-pin" },
    });
    await waitForText(run.host, "source", `MessageActionTriggered act-pin on ${mine.messageId}`);
  });

  test("S30: participants give the count, pages of 100 and every kind by id", async () => {
    const run = await start();
    const { chatUser } = run.host;
    // Before the declaration the host shows two participants and asks nothing.
    expect(await chatUser.participants("host", { tag: "Count" })).toEqual({ tag: "Count", value: { count: 2 } });

    await command(run, "/participants 150");
    expect(await chatUser.participants("source", { tag: "Count" })).toEqual({
      tag: "Count",
      value: { count: 154 },
    });
    const first = await chatUser.participants("source", { tag: "Page", value: { pageSize: 100 } });
    if (first.tag !== "Page") throw new Error(`page gave ${first.tag}`);
    expect(first.value.participants).toHaveLength(100);
    expect(first.value.next).toBe("100");
    const second = await chatUser.participants("source", {
      tag: "Page",
      value: { cursor: first.value.next, pageSize: 100 },
    });
    if (second.tag !== "Page") throw new Error(`page gave ${second.tag}`);
    expect(second.value.participants).toHaveLength(54);
    expect(second.value.next).toBeUndefined();

    const byIds = await chatUser.participants("source", {
      tag: "ByIds",
      value: { participantIds: ["p1", "p2", "p3", "p4", "p5", "p999"] },
    });
    if (byIds.tag !== "Resolved") throw new Error(`by ids gave ${byIds.tag}`);
    expect(byIds.value.participants.map(({ participant }) => participant.tag)).toEqual([
      "Person",
      "LitePerson",
      "Contact",
      "Product",
      "Custom",
    ]);

    // Each kind keeps its host entry beside the product's Mention (S31 on the
    // phone hosts; the test host has no sheet to draw).
    const menus = await Promise.all(
      byIds.value.participants.map((entry) => chatUser.participantMenu("source", entry)),
    );
    expect(menus.map(({ builtIn }) => builtIn)).toEqual([
      "OpenProfile",
      "CopyAddress",
      "OpenContact",
      "OpenProduct",
      undefined,
    ]);
    expect(menus.every(({ actions }) => actions.length === 1 && actions[0]!.actionId === ACTION_MENTION.actionId)).toBe(
      true,
    );
  });

  test("S32: who reacted comes from reactions_query, and NotAvailable hides it", async () => {
    const run = await start();
    const { host } = run;
    await host.chatUser.send("source", "/post text");
    const posted = await waitForText(host, "source", /^Plain text message/);
    await command(run, `/react ${posted.messageId} 👍`);
    expect(await host.chatUser.react("source", posted.messageId, "👍")).toEqual({ tag: "Applied" });

    // `source` declares reaction details at creation.
    const details = await host.chatUser.whoReacted("source", posted.messageId);
    expect(details).toEqual([
      { participantId: "p1", emoji: "👍" },
      { participantId: "user", emoji: "👍" },
    ]);

    // `na`: the product answers NotAvailable, twice with the retry, and the
    // host hides the affordance.
    await command(run, "/reactions na");
    expect(await host.chatUser.whoReacted("source", posted.messageId)).toBeUndefined();
    const asked = host.chatUser.queries().filter(({ name }) => name === "chatReactionsQuery");
    expect(asked.slice(-2).map(({ result }) => result._unsafeUnwrapErr())).toEqual([
      { tag: "Domain", value: { tag: "NotAvailable" } },
      { tag: "Domain", value: { tag: "NotAvailable" } },
    ]);

    // `off`: the room no longer declares the details, so the host asks nothing.
    await command(run, "/reactions off");
    const before = host.chatUser.queries().length;
    expect(await host.chatUser.whoReacted("source", posted.messageId)).toBeUndefined();
    expect(host.chatUser.queries()).toHaveLength(before);
    await until(() => host.room("source")?.config.reactionDetails === false, "the configuration");
  });
});
