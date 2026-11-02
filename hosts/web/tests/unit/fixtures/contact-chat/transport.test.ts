import { describe, expect, test } from "bun:test";
import { errAsync, okAsync } from "neverthrow";
import {
  FOLLOW_RETRY_MS,
  RETENTION_SECONDS,
  messageExpiry,
  truapiStatements,
} from "../../../../fixtures/products/contact-chat/transport.js";

/**
 * Contact Chat's transport over a product's TrUAPI: it signs as the
 * product's own account at the conversation's raw index through
 * `createProof` and submits what that proved; it follows topics and starts
 * again after the subscription ends; and each expiry rises, as the store
 * requires to replace a post on a channel.
 */

const PRODUCT = "contact-chat.test";
const TOPIC = `0x${"ab".repeat(32)}`;
const CHANNEL = `0x${"cd".repeat(32)}`;
const INDEX = `0x${"11".repeat(32)}`;
const SIGNER = `0x${"5e".repeat(32)}`;
const PROOF = {
  tag: "Sr25519",
  value: { signature: `0x${"00".repeat(64)}`, signer: SIGNER },
};

describe("Contact Chat's transport", () => {
  test("an expiry is the retention ahead in the high 32 bits, and always above the last one", () => {
    const now = Date.UTC(2026, 9, 31, 15, 0, 0);
    const first = messageExpiry(null, now);
    expect(first).toBe(BigInt(now / 1000 + RETENTION_SECONDS) << 32n);
    expect(messageExpiry(first, now)).toBe(first + 1n);
    expect(messageExpiry(first, now + 1000) > first).toBe(true);
  });

  test("names its account and signs as the product's own account at the index, then submits what that proved", async () => {
    const calls: unknown[] = [];
    let refuse = false;
    const statements = truapiStatements(
      {
        account: {
          getAccount: (request: unknown) => {
            calls.push({ getAccount: request });
            return okAsync({ account: { publicKey: SIGNER.toUpperCase() } });
          },
        },
        statementStore: {
          createProof: (request: unknown) => {
            calls.push({ createProof: request });
            return okAsync({ proof: PROOF });
          },
          submit: (statement: unknown) => {
            calls.push({ submit: statement });
            return refuse
              ? errAsync({ tag: "Domain", value: { reason: "no allowance" } })
              : okAsync(undefined);
          },
        },
      } as never,
      PRODUCT,
      () => () => {},
    );
    expect(await statements.account(INDEX)).toBe(SIGNER);
    await statements.post(
      INDEX,
      { topics: [TOPIC], channel: CHANNEL, data: "0x01" },
      7n,
    );
    const account = {
      dotNsIdentifier: PRODUCT,
      derivationIndex: { tag: "Raw", value: INDEX },
    };
    const statement = {
      expiry: 7n,
      channel: CHANNEL,
      topics: [TOPIC],
      data: "0x01",
    };
    expect(calls).toEqual([
      { getAccount: { productAccountId: account } },
      { createProof: { productAccountId: account, statement } },
      { submit: { ...statement, proof: PROOF } },
    ]);
    refuse = true;
    // The reason the core gave, not the error's raw value.
    await expect(
      statements.post(
        INDEX,
        { topics: [TOPIC], channel: CHANNEL, data: "0x01" },
        8n,
      ),
    ).rejects.toThrow(/^no allowance$/);
  });

  test("a refused submission says the reason its error carries, else its kind, never the error's value", async () => {
    let error: unknown = null;
    const statements = truapiStatements(
      {
        statementStore: {
          createProof: () => okAsync({ proof: PROOF }),
          submit: () => errAsync(error),
        },
      } as never,
      PRODUCT,
      () => () => {},
    );
    const refusal = async (value: unknown) => {
      error = value;
      return statements
        .post(INDEX, { topics: [TOPIC], channel: CHANNEL, data: "0x01" }, 9n)
        .then(
          () => "posted",
          (thrown: Error) => thrown.message,
        );
    };
    const said: string[] = [];
    for (const value of [
      {
        tag: "Domain",
        value: {
          tag: "V1",
          value: { reason: "statement-store submit failed: refused" },
        },
      },
      { tag: "Denied" },
      { secret: "0x01" },
    ])
      said.push(await refusal(value));
    expect(said).toEqual([
      "statement-store submit failed: refused",
      "the host answered Denied",
      "unknown error",
    ]);
  });

  test("follows the topics through TrUAPI, and starts again after the subscription ends", () => {
    const observers: {
      next(page: unknown): void;
      error(error: unknown): void;
    }[] = [];
    const requests: unknown[] = [];
    const tasks: { task: () => void; delayMs: number }[] = [];
    const statements = truapiStatements(
      {
        statementStore: {
          subscribe: (request: unknown) => {
            requests.push(request);
            return {
              subscribe: (observer: (typeof observers)[number]) => {
                observers.push(observer);
                return { unsubscribe: () => {} };
              },
            };
          },
        },
      } as never,
      PRODUCT,
      (task, delayMs) => {
        tasks.push({ task, delayMs });
        return () => {};
      },
    );
    const heard: unknown[] = [];
    const problems: string[] = [];
    statements.follow(
      [TOPIC],
      (page) => heard.push(...page),
      (reason) => problems.push(reason),
    );
    observers[0]!.next({
      statements: [
        {
          proof: {
            tag: "Sr25519",
            value: { signature: "0x00", signer: `0x${"CD".repeat(32)}` },
          },
          topics: [TOPIC],
          data: "0x02",
        },
      ],
      isComplete: true,
    });
    expect(heard).toEqual([
      { signer: `0x${"cd".repeat(32)}`, topics: [TOPIC], data: "0x02" },
    ]);
    observers[0]!.error(new Error("the connection closed"));
    expect([problems, tasks.map((task) => task.delayMs)]).toEqual([
      ["the connection closed"],
      [FOLLOW_RETRY_MS],
    ]);
    tasks[0]!.task();
    expect(requests).toEqual([
      { request: { tag: "MatchAny", value: [TOPIC] } },
      { request: { tag: "MatchAny", value: [TOPIC] } },
    ]);
  });
});
