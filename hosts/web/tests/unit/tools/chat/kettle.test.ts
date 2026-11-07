import { describe, expect, test } from "bun:test";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import {
  untilFinalizedAllowance,
  type FinalizedAllowanceOptions,
} from "../../../../tools/chat/kettle.js";
import type { Grant } from "../../../../tools/statements/actions.js";

const CLAIM: StatementAccountClaim = {
  account: `0x${"ab".repeat(32)}`,
  period: 20762,
  outcome: "registered",
  seq: 4,
  collection: "LitePeople",
  blockHash: `0x${"0c".repeat(32)}`,
};
const GRANT = { maxCount: 2, maxSize: 512_000 };

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
  test("resolves at the first finalized read that holds the allowance, after the reads that did not", async () => {
    const { options } = scripted([null, new Error("socket closed"), GRANT]);
    expect(await untilFinalizedAllowance(CLAIM, options)).toEqual({
      account: CLAIM.account,
      claimBlock: CLAIM.blockHash,
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
