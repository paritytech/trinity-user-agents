import { describe, expect, it } from "bun:test";
import {
  createLoopbackStatements,
  decodeStatement,
  encodeStatement,
  TOPIC_FIELD_TAGS,
} from "./loopback-statements.js";

const TOPIC_A = `0x${"aa".repeat(32)}`;
const TOPIC_B = `0x${"bb".repeat(32)}`;

/**
 * A statement carrying `topics`, encoded the way the core sends it.
 *
 * Built with the store's own encoder, which the pinned real submission below
 * holds to the wire format. Hand-rolled bytes would let this file exercise the
 * topic filter against a shape no product ever submits.
 */
function statement(topics: string[]): string {
  return encodeStatement({ topics });
}

function subscribe(store: ReturnType<typeof createLoopbackStatements>, filter: unknown) {
  const frames: string[] = [];
  const respond = (frame: string) => frames.push(frame);
  store.handle(
    JSON.stringify({ jsonrpc: "2.0", id: "s1", method: "statement_subscribeStatement", params: [filter] }),
    respond,
  );
  return { frames, respond };
}

function delivered(frames: string[]) {
  return frames.filter((f) => f.includes("newStatements")).length;
}

describe("the in-page statement store", () => {
  it("accepts a submission, because the core rejects anything but new/known", () => {
    const store = createLoopbackStatements();
    const replies: string[] = [];
    const handled = store.handle(
      JSON.stringify({ jsonrpc: "2.0", id: 1, method: "statement_submit", params: [statement([])] }),
      (f) => replies.push(f),
    );
    expect(handled).toBe(true);
    // A bare "new" string is refused by the core as `not accepted`; it reads
    // `.status` off an object.
    expect(JSON.parse(replies[0]!).result).toEqual({ status: "new" });
  });

  it("delivers a submission to a subscriber watching its topic", () => {
    const store = createLoopbackStatements();
    const { frames, respond } = subscribe(store, { matchAll: [TOPIC_A] });
    store.handle(
      JSON.stringify({ id: 2, method: "statement_submit", params: [statement([TOPIC_A])] }),
      respond,
    );
    expect(delivered(frames)).toBe(1);
  });

  it("does not deliver to a subscriber watching a different topic", () => {
    // Without this, a store that fans out to everyone would look correct.
    const store = createLoopbackStatements();
    const { frames, respond } = subscribe(store, { matchAll: [TOPIC_B] });
    store.handle(
      JSON.stringify({ id: 3, method: "statement_submit", params: [statement([TOPIC_A])] }),
      respond,
    );
    expect(delivered(frames)).toBe(0);
  });

  it("matchAny needs one topic where matchAll needs them all", () => {
    const store = createLoopbackStatements();
    const any = subscribe(store, { matchAny: [TOPIC_A, TOPIC_B] });
    const all = subscribe(store, { matchAll: [TOPIC_A, TOPIC_B] });
    store.handle(
      JSON.stringify({ id: 4, method: "statement_submit", params: [statement([TOPIC_A])] }),
      any.respond,
    );
    expect(delivered(any.frames)).toBe(1);
    expect(delivered(all.frames)).toBe(0);
  });

  it("stops delivering after unsubscribe", () => {
    const store = createLoopbackStatements();
    const { frames, respond } = subscribe(store, { matchAll: [] });
    const id = JSON.parse(frames[0]!).result as string;
    store.handle(
      JSON.stringify({ id: 5, method: "statement_unsubscribeStatement", params: [id] }),
      respond,
    );
    store.handle(
      JSON.stringify({ id: 6, method: "statement_submit", params: [statement([TOPIC_A])] }),
      respond,
    );
    expect(delivered(frames)).toBe(0);
  });

  it("leaves everything else to the caller", () => {
    const store = createLoopbackStatements();
    // A chain read must still reach the chain, or serving statements locally
    // would silently blind every other call on that connection.
    expect(
      store.handle(
        JSON.stringify({ id: 7, method: "state_getStorage", params: ["0x00"] }),
        () => {},
      ),
    ).toBe(false);
  });

  it("records submissions and forgets them on clear", () => {
    const store = createLoopbackStatements();
    const encoded = statement([TOPIC_A]);
    store.handle(
      JSON.stringify({ id: 8, method: "statement_submit", params: [encoded] }),
      () => {},
    );
    expect(store.submitted().map((entry) => entry.encoded)).toEqual([encoded]);
    // Provenance travels with the statement, because `submitted()` is the
    // narrowing of everything retained and has to tell the two apart.
    expect(store.submitted()[0]?.fromProduct).toBe(true);
    expect(store.submitted()[0]?.timestamp).toBeGreaterThan(0);

    store.clear();
    expect(store.submitted()).toEqual([]);
    expect(store.statements()).toEqual([]);
  });

  it("does not count an injection as a submission", () => {
    // `submitted()` is how a suite sees what the product sent. Counting an
    // injection there lets a test assert the product submitted something while
    // the host was the only one that acted.
    const store = createLoopbackStatements();
    const { frames } = subscribe(store, null);
    const afterSubscribe = frames.length;
    store.inject(statement([TOPIC_A]));

    expect(store.submitted()).toEqual([]);
    // Still delivered, so this is about the record and not a dropped statement.
    expect(frames.length).toBe(afterSubscribe + 1);
  });

  it("replays an injection to a subscription opened after it", () => {
    // What retaining is for: a suite that injects before its product subscribes
    // would otherwise see nothing, with no way to tell that from a delivery
    // the filter dropped.
    const store = createLoopbackStatements();
    store.inject(statement([TOPIC_A]));
    const { frames } = subscribe(store, { matchAll: [TOPIC_A] });
    expect(delivered(frames)).toBe(1);
  });

  it("does not replay the product's own submission back to it", () => {
    // A store hands a new subscriber what someone else published. Replaying
    // the submitter's own backlog lets a suite waiting for a peer's statement
    // pass on the echo of the one it sent.
    const store = createLoopbackStatements();
    store.handle(
      JSON.stringify({
        id: 9,
        method: "statement_submit",
        params: [statement([TOPIC_A])],
      }),
      () => {},
    );
    const { frames } = subscribe(store, { matchAll: [TOPIC_A] });
    expect(delivered(frames)).toBe(0);
  });
});

describe("the statement encoding the store actually sees", () => {
  // Captured off the wire: a real `statement_submit` the core sent for a
  // product publishing with two topics. Pinned as bytes because the store's
  // whole job is to read what the core sends, and a codec that round-trips
  // only with itself would pass every test here while matching no statement a
  // product ever submitted.
  const REAL_SUBMISSION =
    "0x140000fac74972d39b6b3198e45cd39faf088872a2a96ac858d961f466e8cf7c26e43d" +
    "80b9e42e0420a6b31cee2dea96cff5839c965815aa54e03efb96fd7d83cb7085" +
    "de9845986b90a2ea5f63738e04571be4cf7caee1b9432d7a4b814e7301b5e32c" +
    "025ff4e2cdf3aeb36a04" +
    "2f7274de70c46820efe1c85ec511c903eda53fcdbf33dee19447aafaa51f3789" +
    "05cca68fb1ebe1cb2e0db41280715254aca32521e6be9699418624e3c802d7f6ac" +
    "0805017b2274797065223a2274657374222c2274657874223a22746f70696332" +
    "206d657373616765222c2274696d657374616d70223a313739303136303539373639357d";

  it("decodes both topics of a statement a product really submitted", () => {
    const fields = decodeStatement(REAL_SUBMISSION);
    expect(fields).toBeDefined();
    const topics = fields!
      .filter((field) => TOPIC_FIELD_TAGS.includes(field.tag))
      .map((field) => String(field.value));
    expect(topics).toEqual([
      "0x2f7274de70c46820efe1c85ec511c903eda53fcdbf33dee19447aafaa51f3789",
      "0xcca68fb1ebe1cb2e0db41280715254aca32521e6be9699418624e3c802d7f6ac",
    ]);
  });

  it("decodes the payload a product really submitted", () => {
    const data = decodeStatement(REAL_SUBMISSION)!.find(
      (field) => field.tag === "Data",
    );
    expect(data).toBeDefined();
    const text = new TextDecoder().decode(
      Uint8Array.from(
        (String(data!.value).slice(2).match(/../g) ?? []).map((byte) =>
          parseInt(byte, 16),
        ),
      ),
    );
    expect(JSON.parse(text)).toMatchObject({ type: "test" });
  });

  it("encodes what it can decode", () => {
    const topics = [`0x${"11".repeat(32)}`, `0x${"22".repeat(32)}`];
    const fields = decodeStatement(encodeStatement({ topics, data: "0x6869" }));
    expect(
      fields!
        .filter((field) => TOPIC_FIELD_TAGS.includes(field.tag))
        .map((field) => String(field.value)),
    ).toEqual(topics);
    expect(fields!.find((field) => field.tag === "Data")?.value).toBe("0x6869");
  });

  it("refuses a fifth topic rather than dropping it", () => {
    // The store has four topic fields. Silently keeping the first four would
    // read as a filter that failed to match.
    expect(() =>
      encodeStatement({ topics: Array(5).fill(`0x${"11".repeat(32)}`) }),
    ).toThrow(/at most 4 topics/);
  });
});
