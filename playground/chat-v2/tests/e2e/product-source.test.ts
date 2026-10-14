/**
 * The product-source room `source` of the sample product on the test host:
 * the host asks the product for pages and hands it the user's actions.
 * S14 (seen), S15, S16, S18, S19, S20, S21 and S46 of 04-TESTING §4.
 */
import { afterEach, describe, expect, test } from "bun:test";
import type { ChatMessage, ProductChatMessagesQueryRequest } from "@parity/truapi";
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

/** Send a command in `source` and wait for its `ok` reply. */
async function command(run: Running, line: string): Promise<void> {
  const outcome = await run.host.chatUser.send("source", line);
  expect(outcome.tag).toBe("Posted");
  await waitForText(run.host, "source", `ok ${line}`);
}

/** The anchors of the pages the host asked for, as `Before:<cursor>`. */
function pageAnchors(run: Running): string[] {
  return run.host.chatUser
    .queries()
    .filter(({ name }) => name === "chatMessagesQuery")
    .map(({ request }) => {
      const { anchor } = request as ProductChatMessagesQueryRequest;
      return `${anchor.tag}:${anchor.value.cursor ?? ""}`;
    });
}

function isSorted(messages: ChatMessage[]): boolean {
  return messages.every((message, index) => index === 0 || messages[index - 1]!.timestamp <= message.timestamp);
}

suite("product-source room on the test host", () => {
  test("S15, S16: the room opens on the newest page and pages older until the end, no page twice", async () => {
    const run = await start();
    const { host } = run;
    await host.chatUser.send("source", "/history 120");
    await until(() => run.product.store.messages().length === 120, "the history");
    await run.product.idle();

    await host.chatUser.close("source");
    const opened = await host.chatUser.open("source");
    // The newest page, oldest first: the newest message is at the bottom.
    expect(opened.at(-1)?.messageId).toBe("source-h-00120");
    expect(opened).toHaveLength(40);
    expect(isSorted(opened)).toBe(true);

    let pages = 1;
    while (await host.chatUser.loadOlder("source")) pages++;
    expect(pages).toBe(3);
    // Past the oldest page the host asks nothing more.
    expect(await host.chatUser.loadOlder("source")).toBe(false);

    const feed = host.feed("source");
    expect(feed).toHaveLength(120);
    expect(feed[0]!.messageId).toBe("source-h-00001");
    expect(isSorted(feed)).toBe(true);
    const anchors = pageAnchors(run).slice(-3);
    expect(anchors[0]).toBe("Before:");
    expect(new Set(anchors).size).toBe(anchors.length);
  });

  test("S18: feed updates apply to the screen: new in time order, edit, reaction, status, delete", async () => {
    const run = await start();
    const { host } = run;
    await host.chatUser.send("source", "/post text");
    const posted = await waitForText(host, "source", /^Plain text message/);

    await command(run, `/edit ${posted.messageId} changed`);
    await command(run, `/react ${posted.messageId} 🎉`);
    await command(run, `/status ${posted.messageId} read`);
    const shown = () => host.feed("source").find(({ messageId }) => messageId === posted.messageId);
    expect(shown()).toMatchObject({
      body: { tag: "Text", value: { text: "changed" } },
      status: "Read",
      reactions: [{ emoji: "🎉", count: 1, reactedByUser: false }],
    });
    expect(shown()?.editedAt).toBeDefined();

    await command(run, `/delete ${posted.messageId}`);
    expect(shown()).toBeUndefined();
    expect(isSorted(host.feed("source"))).toBe(true);
    // Nothing of a product-source room is stored by the host.
    expect(host.room("source")?.updates.every(({ persist }) => !persist)).toBe(true);
  });

  test("S19: a send shows Preparing, then the message of MessageCreated replaces it", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/slow 300");

    const sending = host.chatUser.send("source", "hello");
    const pending = await until(
      () => host.feed("source").find(({ status }) => status === "Preparing"),
      "the optimistic message",
    );
    expect(pending.messageId).toMatch(/^pending:/);
    const outcome = await sending;
    if (outcome.tag !== "Posted") throw new Error(`send gave ${outcome.tag}`);
    expect(outcome.message.messageId).toMatch(/^source-u-\d{4}$/);
    expect(outcome.message.status).toBe("Sent");
    const feed = host.feed("source");
    expect(feed.some(({ messageId }) => messageId === pending.messageId)).toBe(false);
    expect(feed.filter(({ messageId }) => messageId === outcome.message.messageId)).toHaveLength(1);
  });

  test("S20: Rejected removes the message and shows the reason", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/fail reject");

    const outcome = await host.chatUser.send("source", "refused text");
    expect(outcome).toEqual({ tag: "Rejected", reason: "Rejected by /fail" });
    expect(host.feed("source").some((message) => message.body.tag === "Text" && message.body.value.text === "refused text")).toBe(false);
    expect(host.chatUser.notices()).toContainEqual({ roomId: "source", text: "Rejected by /fail" });
  });

  test("S21: no answer in time marks the message Failed, and retry sends it again", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/fail timeout");
    // The RFC waits 30 s; the suite waits less.
    host.chatUser.setTimings({ newMessage: 300 });

    const outcome = await host.chatUser.send("source", "lost text");
    if (outcome.tag !== "Failed") throw new Error(`send gave ${outcome.tag}`);
    expect(outcome.refusal).toEqual({ tag: "Timeout" });
    const failed = () => host.feed("source").find(({ messageId }) => messageId === outcome.messageId);
    expect(failed()?.status).toBe("Failed");
    expect((await host.chatUser.messageMenu("source", outcome.messageId)).builtIn).toContain("Retry");
    // The product logs the stopped query as `Timeout`.
    await until(() => run.log().some((line) => line.via === "answer" && line.kind === "Timeout"), "the Timeout line");

    expect(await host.chatUser.retry("source", outcome.messageId)).toEqual({ tag: "Applied" });
    await until(() => failed()?.status === "Sent", "the status Sent after the retry");
  });

  test("S14: report_seen gives the product Seen, and the product clears the badge", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/preview badge 3");
    expect(host.preview("source")?.badge).toEqual({ tag: "UnreadCount", value: { count: 3 } });

    expect(await host.chatUser.seen("source")).toEqual({ tag: "Applied" });
    expect(host.room("source")?.preview?.badge).toEqual({ tag: "UnreadCount", value: { count: 0 } });
    const seen = run.log().filter((line) => line.via === "user_action" && line.kind === "Seen");
    expect(seen).toHaveLength(1);
  });

  test("S46: a NotAvailable answer is asked again once after the delay", async () => {
    const run = await start();
    const { host } = run;
    await command(run, "/fail unavailable");
    await host.chatUser.close("source");

    const startedAt = Date.now();
    const opened = await host.chatUser.open("source");
    expect(Date.now() - startedAt).toBeGreaterThanOrEqual(100);
    const pages = host.chatUser.queries().filter(({ name }) => name === "chatMessagesQuery").slice(-2);
    expect(pages[0]!.result._unsafeUnwrapErr()).toEqual({ tag: "Domain", value: { tag: "NotAvailable" } });
    expect(pages[1]!.result.isOk()).toBe(true);
    expect(opened.length).toBeGreaterThan(0);
    // `NotAvailable` is not a failure, so the screen shows no error line.
    expect(host.chatUser.notices()).toEqual([]);
  });
});
