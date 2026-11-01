import { describe, expect, test } from "bun:test";
import {
  cryptoRandom,
  toHex,
  utf8,
} from "../../../../src/invitations/sealing.js";
import {
  TEXT_MAX_CHARS,
  WINDOW_MAX_BYTES,
  WINDOW_MESSAGES,
  simpleText,
} from "../../../../fixtures/products/contact-chat/simple-text.js";

/**
 * `simple-text/1` on its own: what each side posts and reads. The setup is
 * authenticated by the hosts, but topics are public once used, so a side
 * must read only its peer's account on its own topic.
 */

const ALICE = toHex(cryptoRandom(32));
const BOB = toHex(cryptoRandom(32));
const MALLORY = toHex(cryptoRandom(32));

function connected() {
  const invited = simpleText.invite(ALICE, cryptoRandom);
  const made = simpleText.respond(invited.payload, BOB, cryptoRandom)!;
  const opened = simpleText.open(invited.pending, made.response);
  if (!opened.ok) throw new Error(opened.reason);
  return { alice: opened.value, bob: made.session };
}

const text = (body: string, at = 1_000) => ({
  id: toHex(cryptoRandom(16)),
  text: body,
  at,
});

describe("simple-text/1", () => {
  test("a post carries Alice's messages on her topic and channel; Bob reads them, and Alice does not read her own", () => {
    const { alice, bob } = connected();
    const messages = [text("Hello"), text("Again", 2_000)];
    const { post, carried } = simpleText.post(alice, messages);
    expect([post.topics, post.channel, carried]).toEqual([
      [bob.listen],
      alice.channel,
      messages.map((message) => message.id),
    ]);
    expect(alice.channel).not.toBe(bob.channel);
    expect(simpleText.read(bob, { signer: ALICE, ...post })).toEqual(messages);
    expect(simpleText.read(alice, { signer: ALICE, ...post })).toEqual([]);
  });

  test("Bob reads nothing another account posts on his topic, or Alice posts elsewhere, or that is not a window", () => {
    const { alice, bob } = connected();
    const { post } = simpleText.post(alice, [text("Hello")]);
    expect(simpleText.read(bob, { signer: MALLORY, ...post })).toEqual([]);
    expect(
      simpleText.read(bob, {
        signer: ALICE,
        topics: [toHex(cryptoRandom(32))],
        data: post.data,
      }),
    ).toEqual([]);
    for (const data of [
      "0x",
      toHex(utf8("not json")),
      toHex(utf8(JSON.stringify({ messages: [text("no version")] }))),
    ])
      expect(
        simpleText.read(bob, { signer: ALICE, topics: [bob.listen], data }),
      ).toEqual([]);
  });

  test("a post carries at most the last WINDOW_MESSAGES messages, and fewer when they would pass WINDOW_MAX_BYTES", () => {
    const { alice } = connected();
    const many = Array.from({ length: WINDOW_MESSAGES + 3 }, (_, index) =>
      text(`Message ${index}`),
    );
    expect(simpleText.post(alice, many).carried).toEqual(
      many.slice(-WINDOW_MESSAGES).map((message) => message.id),
    );
    const long = Array.from({ length: 12 }, () =>
      text("a".repeat(TEXT_MAX_CHARS)),
    );
    const { post, carried } = simpleText.post(alice, long);
    expect(carried.length).toBeLessThan(long.length);
    expect((post.data.length - 2) / 2).toBeLessThanOrEqual(WINDOW_MAX_BYTES);
    expect(carried).toEqual(
      long.slice(-carried.length).map((message) => message.id),
    );
  });

  test("a response for another invitation, or with one topic for both sides, does not open", () => {
    const invited = simpleText.invite(ALICE, cryptoRandom);
    const other = simpleText.invite(ALICE, cryptoRandom);
    const made = simpleText.respond(other.payload, BOB, cryptoRandom)!;
    expect(simpleText.open(invited.pending, made.response)).toEqual({
      ok: false,
      reason: "the response answers another invitation",
    });
    const same = simpleText.respond(invited.payload, BOB, () =>
      new Uint8Array(32).fill(7),
    )!;
    expect(simpleText.open(invited.pending, same.response)).toEqual({
      ok: false,
      reason: "the response gave one topic for both sides",
    });
  });

  test("an invitation it cannot use is not answered, and a text too long is not posted", () => {
    expect(simpleText.respond("0x00", BOB, cryptoRandom)).toBeNull();
    const { alice } = connected();
    expect(() =>
      simpleText.post(alice, [text("a".repeat(TEXT_MAX_CHARS + 1))]),
    ).toThrow(`at most ${TEXT_MAX_CHARS} characters`);
  });
});
