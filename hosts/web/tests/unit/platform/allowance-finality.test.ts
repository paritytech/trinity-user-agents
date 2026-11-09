import { describe, expect, test } from "bun:test";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import {
  untilFinalizedAllowance,
  finalizedOrWhyNot,
  readClaimGrant,
  connectAllowanceRead,
  type FinalizedAllowanceOptions,
  type Grant,
} from "../../../src/platform/allowance-finality.js";

const CLAIM: StatementAccountClaim = {
  account: `0x${"ab".repeat(32)}`,
  period: 20762,
  outcome: "registered",
  seq: 4,
  collection: "LitePeople",
  blockHash: `0x${"0c".repeat(32)}`,
};
const GRANT = { maxCount: 2, maxSize: 512_000 };

test("session end cancels a pending connection and closes it if it arrives late", async () => {
  let answer!: (
    connection: import("@parity/truapi-host").JsonRpcConnection,
  ) => void;
  let closed = false;
  const session = new AbortController();
  const opening = connectAllowanceRead(
    {
      genesis: "0x00",
      chain: {
        connect: () =>
          new Promise((resolve) => {
            answer = resolve;
          }),
      },
    },
    session.signal,
  );
  session.abort();
  await expect(opening).rejects.toThrow("stopped");
  answer({
    send: () => {
      throw new Error("no read after session end");
    },
    async *responses() {},
    close: () => {
      closed = true;
    },
  });
  await Promise.resolve();
  expect(closed).toBe(true);
});

/** Options whose reads answer `answers` in turn, the last one ever after, on a clock that each sleep moves. */
function scripted(
  answers: (Grant | Error)[],
  overrides: Partial<FinalizedAllowanceOptions> = {},
) {
  let clock = 0;
  let reads = 0;
  const options: FinalizedAllowanceOptions = {
    read: async () => {
      const answer = answers[Math.min(reads, answers.length - 1)]!;
      reads += 1;
      if (answer instanceof Error) throw answer;
      return { block: `0xf${reads}`, grant: answer };
    },
    sleep: async (ms) => {
      clock += ms;
    },
    now: () => clock,
    running: () => true,
    intervalMs: 3_000,
    timeoutMs: 30_000,
    ...overrides,
  };
  return { options, reads: () => reads };
}

describe("waiting for a conversation account's allowance at the finalized head", () => {
  test("yesterday's positive grant cannot prove today's known claim finalized, and a reorg stops the watch", async () => {
    let reads = 0;
    const { options } = scripted([GRANT], {
      requireClaimFinality: true,
      read: async () => ({
        block: "0xf1",
        grant: GRANT,
        claimFinal: false,
        claimLost: ++reads === 2,
      }),
    });
    expect(await finalizedOrWhyNot(CLAIM, options)).toMatchObject({
      ended: false,
      why: expect.stringContaining("not on finalized ancestry"),
    });
    expect(reads).toBe(2);
  });

  test("an unknown inclusion hash proves only finalized allowance, never claim finality", async () => {
    const { options } = scripted([GRANT]);
    expect(
      await untilFinalizedAllowance(
        { ...CLAIM, blockHash: undefined },
        options,
      ),
    ).toMatchObject({ claimFinal: false });
  });

  test("a session ended during a positive read cannot settle successfully", async () => {
    let running = true;
    const { options } = scripted([GRANT], {
      running: () => running,
      read: async () => {
        running = false;
        return { block: "0xf1", grant: GRANT };
      },
    });
    expect(await finalizedOrWhyNot(CLAIM, options)).toMatchObject({
      ended: true,
    });
  });
  test("resolves at the first finalized read that holds the allowance, after the reads that did not", async () => {
    const { options } = scripted([null, new Error("socket closed"), GRANT]);
    expect(await untilFinalizedAllowance(CLAIM, options)).toEqual({
      account: CLAIM.account,
      claimBlock: CLAIM.blockHash,
      claimFinal: false,
      seq: 4,
      collection: "LitePeople",
      finalizedBlock: "0xf3",
      grant: GRANT,
      waitedMs: 6_000,
      reads: 2,
    });
  });

  test("a zero allowance is no allowance", async () => {
    const { options } = scripted([{ maxCount: 0, maxSize: 0 }, GRANT]);
    expect((await untilFinalizedAllowance(CLAIM, options)).reads).toBe(2);
  });

  test("gives up after its timeout, saying which slot is not final and that nothing was posted", async () => {
    const { options, reads } = scripted([null]);
    const waiting = untilFinalizedAllowance(CLAIM, options);
    await expect(waiting).rejects.toThrow(
      `This chat's account ${CLAIM.account} has its slot (LitePeople 4, block ${CLAIM.blockHash}), but it was not final after 30 s: finalized block 0xf11 has no allowance for it yet. Nothing was posted; send again to retry.`,
    );
    expect(reads()).toBe(11);
  });

  test("a product names the account and the next step in its own words", async () => {
    const { options } = scripted([null]);
    await expect(
      untilFinalizedAllowance(CLAIM, {
        ...options,
        wording: {
          account: "Nova Chat's account",
          retry: "it may be tried again",
        },
      }),
    ).rejects.toThrow(
      `Nova Chat's account ${CLAIM.account} has its slot (LitePeople 4, block ${CLAIM.blockHash}), but it was not final after 30 s: finalized block 0xf11 has no allowance for it yet. Nothing was posted; it may be tried again.`,
    );
  });

  test("stops when the session ends", async () => {
    let running = true;
    const { options } = scripted([null], {
      running: () => running,
      sleep: async () => {
        running = false;
      },
    });
    await expect(untilFinalizedAllowance(CLAIM, options)).rejects.toThrow(
      "The session ended before the chat's allowance was final.",
    );
  });
});

test("RPC finality checks the canonical hash at the claim height against a newer finalized block", async () => {
  for (const canonical of [CLAIM.blockHash!, "0xother"]) {
    const sent: { method: string; params: unknown[] }[] = [];
    let closed = false;
    const read = await readClaimGrant(
      {
        genesis: `0x${"01".repeat(32)}`,
        chain: {
          connect: async () => ({
            send: (text) => {
              sent.push(JSON.parse(text));
            },
            async *responses() {
              for (const message of [
                { id: 1, result: "0xfinal" },
                { id: 2, result: { number: "0x10" } },
                { id: 3, result: { number: "0x20" } },
                { id: 4, result: canonical },
                { id: 5, result: "0x0100000000280000" },
              ])
                yield JSON.stringify(message);
            },
            close: () => {
              closed = true;
            },
          }),
        },
      },
      CLAIM.account,
      CLAIM.blockHash!,
    );
    expect(read).toEqual({
      block: "0xfinal",
      grant: { maxCount: 1, maxSize: 10_240 },
      claimFinal: canonical === CLAIM.blockHash,
      claimLost: canonical !== CLAIM.blockHash,
    });
    expect(
      sent.find((call) => call.method === "state_getStorage")?.params[1],
    ).toBe("0xfinal");
    expect(
      sent.find((call) => call.method === "chain_getBlockHash")?.params,
    ).toEqual(["0x10"]);
    expect(closed).toBe(true);
  }
});
