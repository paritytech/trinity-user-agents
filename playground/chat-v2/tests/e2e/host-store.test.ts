/**
 * The host-store room `host` of the sample product on the test host: the host
 * keeps the messages, and the user's actions reach the product as events.
 * S9, S10, S12, S13, S14 (commands) and S38 of 04-TESTING §4.
 */
import { afterEach, describe, expect, test } from "bun:test";
import type { ChatMessage } from "@parity/truapi";
import {
  newestProductMessage,
  startOnTestHost,
  testHostIsBuilt,
  texts,
  until,
  waitForText,
  type Running,
} from "../support/test-host";

const suite = testHostIsBuilt() ? describe : describe.skip;

let running: Running | undefined;
async function start() {
  running = await startOnTestHost();
  return running;
}
afterEach(() => {
  running?.dispose();
  running = undefined;
});

/** Send a command in `host` and wait for its `ok` reply. */
async function command(run: Running, line: string): Promise<void> {
  await run.host.chatUser.send("host", line);
  await waitForText(run.host, "host", `ok ${line}`);
}

/** Send `/post text` and give the message it posts. */
async function postText(run: Running): Promise<ChatMessage> {
  const before = run.host.feed("host").length;
  await run.host.chatUser.send("host", "/post text");
  return until(
    () =>
      run.host
        .feed("host")
        .slice(before)
        .find((message) => message.origin.participantId === "product"),
    "the posted text",
  );
}

function eventTags(run: Running): string[] {
  return run.host.events().map(({ event }) => event.tag);
}

suite("host-store room on the test host", () => {
  test("S44: /flood sends 25 edits at once and the core refuses the ones past the budget", async () => {
    const run = await start();
    const { host } = run;
    await host.chatUser.send("host", "/flood");
    const reply = await until(
      () => texts(run.host, "host").find((text) => text?.startsWith("ok /flood ")),
      "the /flood reply",
      10_000,
    );
    const [, accepted, refused] = /^ok \/flood accepted=(\d+) refused=(\d+)$/.exec(reply) ?? [];
    // The budget is 20, and it refills at one call each 50 ms, so a slow
    // burst can get one or two more.
    expect(Number(accepted)).toBeGreaterThanOrEqual(20);
    expect(Number(refused)).toBeGreaterThanOrEqual(1);
    expect(Number(accepted) + Number(refused)).toBe(25);
    const target = host.feed("host").find((message) => message.body.tag === "Text" && message.body.value.text === "Flood of 25 edits");
    expect(target?.editedAt).toBeDefined();
    expect(run.log().filter((line) => line.via === "flood")).toEqual([
      expect.objectContaining({ room: "host", kind: "Done", data: { sent: 25, accepted: Number(accepted), refused: Number(refused) } }),
    ]);
  }, 15_000);

  test("S9: Deleted removes the message and the list line recomputes", async () => {
    const run = await start();
    const { host } = run;
    const posted = await postText(run);
    expect(host.preview("host")?.text).toBe(
      posted.body.tag === "Text" ? posted.body.value.text : undefined,
    );

    await command(run, `/delete ${posted.messageId}`);
    expect(host.feed("host").some(({ messageId }) => messageId === posted.messageId)).toBe(false);
    expect(host.room("host")?.updates.at(-2)).toEqual({
      update: { tag: "Deleted", value: { messageId: posted.messageId } },
      persist: true,
    });
    // The newest message is now the `ok` reply, and the list line follows it.
    expect(host.preview("host")?.text).toBe(`ok /delete ${posted.messageId}`);
  });

  test("S10: reactions of p1 and of the user aggregate with reactedByUser, and toggle", async () => {
    const run = await start();
    const { host } = run;
    const posted = await postText(run);
    const reactions = () => host.feed("host").find(({ messageId }) => messageId === posted.messageId)?.reactions;

    await command(run, `/react ${posted.messageId} 👍`);
    expect(reactions()).toEqual([{ emoji: "👍", count: 1, reactedByUser: false }]);

    await host.chatUser.react("host", posted.messageId, "👍");
    expect(reactions()).toEqual([{ emoji: "👍", count: 2, reactedByUser: true }]);

    await command(run, `/unreact ${posted.messageId} 👍`);
    expect(reactions()).toEqual([{ emoji: "👍", count: 1, reactedByUser: true }]);

    await host.chatUser.react("host", posted.messageId, "👍", false);
    expect(reactions()).toEqual([]);
    expect(
      host
        .events()
        .filter(({ event }) => event.tag === "UserReacted")
        .map(({ event }) => event.value),
    ).toEqual([
      { messageId: posted.messageId, emoji: "👍", placed: true },
      { messageId: posted.messageId, emoji: "👍", placed: false },
    ]);
  });

  test("S12: send, reply, edit, delete and react give their events, each echoed", async () => {
    const run = await start();
    const { host, chatUser } = { host: run.host, chatUser: run.host.chatUser };
    await command(run, "/echo");

    const sent = await chatUser.send("host", "hello");
    if (sent.tag !== "Posted") throw new Error(`send gave ${sent.tag}`);
    await waitForText(host, "host", "echo MessagePosted hello");
    const reply = await chatUser.send("host", "a reply", { replyTo: sent.message.messageId });
    if (reply.tag !== "Posted") throw new Error(`reply gave ${reply.tag}`);
    await waitForText(host, "host", "echo MessagePosted a reply");
    await chatUser.edit("host", sent.message.messageId, "hello again");
    await waitForText(host, "host", `echo UserEdited ${sent.message.messageId} hello again`);
    await chatUser.react("host", sent.message.messageId, "❤️");
    await waitForText(host, "host", `echo UserReacted ${sent.message.messageId} ❤️ placed`);
    await chatUser.remove("host", reply.message.messageId);
    await waitForText(host, "host", `echo UserDeleted ${reply.message.messageId}`);

    const events = host.events().filter(({ event }) => event.tag !== "Command");
    expect(events.map(({ event }) => event.tag)).toEqual([
      "MessagePosted",
      "MessagePosted",
      "MessagePosted",
      "UserEdited",
      "UserReacted",
      "UserDeleted",
    ]);
    // A posted message carries the id the host gave it, and a reply its target.
    expect(events[1]!.event).toEqual({
      tag: "MessagePosted",
      value: { messageId: sent.message.messageId, body: { tag: "Text", value: { text: "hello" } } },
    });
    expect(events[2]!.event).toMatchObject({
      tag: "MessagePosted",
      value: { messageId: reply.message.messageId, replyTo: sent.message.messageId },
    });
    expect(events.every(({ peer, roomId }) => peer === "native" && roomId === "host")).toBe(true);
    expect(host.feed("host").find(({ messageId }) => messageId === sent.message.messageId)?.body).toEqual({
      tag: "Text",
      value: { text: "hello again" },
    });
  });

  test("S13: an Actions press gives ActionTriggered, and the edit removes the buttons", async () => {
    const run = await start();
    const { host } = run;
    await host.chatUser.send("host", "/post actions");
    const actions = await until(
      () => host.feed("host").find((message) => message.body.tag === "Actions"),
      "the Actions message",
    );
    if (actions.body.tag !== "Actions") throw new Error("not an Actions body");
    expect(actions.body.value.actions.map(({ actionId }) => actionId)).toEqual(["act-yes", "act-no"]);

    await host.chatUser.pressAction("host", actions.messageId, "act-yes");
    const picked = await until(() => {
      const message = host.feed("host").find(({ messageId }) => messageId === actions.messageId);
      return message?.body.tag === "Actions" && message.body.value.actions.length === 0 ? message : undefined;
    }, "the edited Actions message");
    expect(picked.body).toMatchObject({ tag: "Actions", value: { actions: [] } });
    expect(picked.editedAt).toBeDefined();
    expect(eventTags(run)).toContain("ActionTriggered");
  });

  test("S14: a declared /command gives Command and posts no message", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/commands");
    expect(host.room("host")?.commands.map(({ name }) => name)).toContain("help");
    const before = host.feed("host").length;

    const sent = await host.chatUser.send("host", "/help");
    expect(sent).toEqual({ tag: "Command", command: { command: "help", payload: "" } });
    const help = await until(
      () => host.feed("host").slice(before).find((message) => message.body.tag === "RichText"),
      "the /help answer",
    );
    // The user's text is not a message: only the product's answer arrived.
    expect(host.feed("host").slice(before)).toEqual([help]);
    expect(eventTags(run).at(-1)).toBe("Command");

    // A text that names no declared command is a message.
    const plain = await host.chatUser.send("host", "/nothing here");
    expect(plain.tag).toBe("Posted");
  });

  test("S38: a sendsText reply sends its text, the other gives SuggestionSelected", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/suggest");
    expect(host.room("host")?.suggestions.map(({ suggestionId }) => suggestionId)).toEqual([
      "sg-send",
      "sg-event",
    ]);

    const sent = await host.chatUser.pickSuggestion("host", "sg-send");
    expect(sent?.tag).toBe("Posted");
    expect(texts(host, "host")).toContain("Say hello");

    const before = host.feed("host").length;
    expect(await host.chatUser.pickSuggestion("host", "sg-event")).toBeUndefined();
    expect(run.host.events().at(-1)?.event).toEqual({
      tag: "SuggestionSelected",
      value: { suggestionId: "sg-event" },
    });
    // The product answers the event with `/help`.
    await until(
      () => host.feed("host").slice(before).find((message) => message.body.tag === "RichText"),
      "the /help answer",
    );
    expect(newestProductMessage(host, "host").body.tag).toBe("RichText");
  });
});
