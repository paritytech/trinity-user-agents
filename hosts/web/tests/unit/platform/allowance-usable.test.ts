import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import type {
  ClaimedAccount,
  Grant,
} from "../../../src/platform/allowance-finality.js";
import {
  CHILD_TIMEOUT_MS,
  peopleAllowanceGuard,
  postingRule,
  readAfterChild,
  untilUsableAllowance,
  type BestRead,
  type PostingRule,
  type UsableAllowanceOptions,
} from "../../../src/platform/allowance-usable.js";
import { NETWORKS } from "../../../src/platform/network-config.js";

/**
 * When an account may post after its claim. A node accepts a statement
 * against the allowance at its best block; whether it keeps it while the
 * claim is not final depends on its version, so only a checked network posts
 * at a best block, and only after a child of the claim's block.
 */

const PASEO_PEOPLE =
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";
const BEST: PostingRule = { wait: "best", why: "checked" };
const FINALIZED: PostingRule = { wait: "finalized", why: "not checked" };
const CLAIM_BLOCK = `0x${"0c".repeat(32)}`;
const CLAIM: ClaimedAccount = {
  account: `0x${"ab".repeat(32)}`,
  period: 20766,
  seq: 4,
  collection: "LitePeople",
  blockHash: CLAIM_BLOCK,
};
const GRANT = { maxCount: 1, maxSize: 10_240 };

describe("the posting rule", () => {
  test("Paseo Next v2 posts at a best block, over its People RPC node, on the People chain that was checked", () => {
    expect(postingRule(NETWORKS.paseo, "rpc", PASEO_PEOPLE).wait).toBe("best");
  });

  // Each of these stands on a node that was not checked, so it waits for the finalized head.
  test("PreviewNet, the light client, and another People chain under the same name wait for the finalized head", () => {
    const rules = [
      postingRule(NETWORKS.previewnet, "rpc", PASEO_PEOPLE),
      postingRule(NETWORKS.paseo, "light-client", PASEO_PEOPLE),
      postingRule(NETWORKS.paseo, "rpc", `0x${"11".repeat(32)}`),
    ];
    expect(rules.map((rule) => rule.wait)).toEqual([
      "finalized",
      "finalized",
      "finalized",
    ]);
    expect(rules[1]!.why).toContain("light client");
    expect(rules[2]!.why).toContain("genesis");
  });
});

/**
 * Options over a chain whose finalized reads answer `finalized` in turn, the
 * last one ever after, and whose best-block read answers `best`, on a clock
 * that each sleep moves.
 */
function chain(
  rule: PostingRule,
  best: BestRead | Error,
  finalized: Grant[],
  overrides: Partial<UsableAllowanceOptions> = {},
) {
  let clock = 0;
  const reads = { finalized: 0, best: 0 };
  const options: UsableAllowanceOptions = {
    rule,
    readAfterChild: async (account, claimBlock) => {
      reads.best += 1;
      expect([account, claimBlock]).toEqual([CLAIM.account, CLAIM_BLOCK]);
      if (best instanceof Error) throw best;
      return best;
    },
    read: async () => {
      const grant = finalized[Math.min(reads.finalized, finalized.length - 1)]!;
      reads.finalized += 1;
      return { block: `0xf${reads.finalized}`, grant, claimFinal: true };
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
  return { options, reads };
}

describe("waiting until an account may post", () => {
  test("on a best-block network, posts once the claim's block has a child at the best block that holds the allowance, before it is final", async () => {
    const { options, reads } = chain(
      BEST,
      { tag: "child", block: "0xbe57", grant: GRANT },
      [null, null, GRANT],
    );
    const usable = await untilUsableAllowance(CLAIM, options);
    const { final, ...seen } = usable;
    expect(seen).toEqual({
      account: CLAIM.account,
      seq: 4,
      collection: "LitePeople",
      claimBlock: CLAIM_BLOCK,
      claimFinal: false,
      running: options.running,
      usableAt: "best",
      block: "0xbe57",
      grant: GRANT,
      waitedMs: 0,
    });
    // Behind the post, the claim is followed to the finalized head.
    expect(await final).toBeNull();
    expect(reads).toEqual({ best: 1, finalized: 3 });
  });

  test("says behind the post when the claim did not become final in time", async () => {
    const { options } = chain(
      BEST,
      { tag: "child", block: "0xbe57", grant: GRANT },
      [null],
    );
    expect(await (await untilUsableAllowance(CLAIM, options)).final).toBe(
      "its allowance was not final after 30 s: finalized block 0xf11 has no allowance for it yet",
    );
  });

  test("on a network not checked, never reads a best block and waits for the finalized head", async () => {
    const { options, reads } = chain(
      FINALIZED,
      { tag: "child", block: "0xbe57", grant: GRANT },
      [null, GRANT],
    );
    const usable = await untilUsableAllowance(CLAIM, options);
    expect([usable.usableAt, usable.block, usable.why, reads]).toEqual([
      "finalized",
      "0xf2",
      "not checked",
      { best: 0, finalized: 2 },
    ]);
    expect(await usable.final).toBeNull();
  });

  // The backup, and a post claimed again to recover it, ask for this.
  test("asked for the finalized head, waits for it on a best-block network too", async () => {
    const { options, reads } = chain(
      BEST,
      { tag: "child", block: "0xbe57", grant: GRANT },
      [GRANT],
    );
    const usable = await untilUsableAllowance(CLAIM, options, "finalized");
    expect([usable.usableAt, reads.best]).toEqual(["finalized", 0]);
  });

  test("waits for the finalized head when it cannot place the claim: registered before, no child, no allowance after it, or no best read", async () => {
    const cases: [ClaimedAccount, BestRead | Error][] = [
      [{ ...CLAIM, blockHash: undefined }, { tag: "final" }],
      [CLAIM, { tag: "unseen" }],
      [CLAIM, { tag: "child", block: "0xbe57", grant: null }],
      [
        CLAIM,
        { tag: "child", block: "0xbe57", grant: { maxCount: 0, maxSize: 0 } },
      ],
      [CLAIM, new Error("socket closed")],
      [CLAIM, { tag: "final" }],
    ];
    const why: string[] = [];
    for (const [claim, best] of cases) {
      const usable = await untilUsableAllowance(
        claim,
        chain(BEST, best, [GRANT]).options,
      );
      expect(usable.usableAt).toBe("finalized");
      why.push(usable.why ?? "");
    }
    expect(why).toEqual([
      "the account was registered before, in a block this host was not told",
      `its claim's block ${CLAIM_BLOCK} had no child at the best block within ${CHILD_TIMEOUT_MS / 1000} s`,
      "best block 0xbe57 after its claim holds no allowance for it",
      "best block 0xbe57 after its claim holds no allowance for it",
      "the best block could not be read: socket closed",
      "its claim is final already",
    ]);
  });

  test("a session that ends stops the wait, and the check behind a post says so", async () => {
    let running = true;
    const ended = chain(
      BEST,
      { tag: "child", block: "0xbe57", grant: GRANT },
      [null],
      {
        running: () => running,
        readAfterChild: async () => {
          running = false;
          return { tag: "child", block: "0xbe57", grant: GRANT };
        },
      },
    );
    await expect(untilUsableAllowance(CLAIM, ended.options)).rejects.toThrow(
      "The session ended before the chat's allowance was usable.",
    );

    running = true;
    const behind = chain(
      BEST,
      { tag: "child", block: "0xbe57", grant: GRANT },
      [null],
      {
        running: () => running,
        sleep: async () => {
          running = false;
        },
      },
    );
    const usable = await untilUsableAllowance(CLAIM, behind.options);
    expect(await usable.final).toBe(
      "the session ended before its allowance was final",
    );
  });
});

/**
 * A `chainHead_v1` connection whose follow plays `events` once followed, and
 * answers a storage read with `value` at the blocks `holds` names, with
 * nothing elsewhere.
 */
function following(
  events: object[],
  value: HexString | null,
  holds: (block: string) => boolean = () => true,
  node: { genesis?: string; version?: string; methods?: string[] } = {},
) {
  const sent: Record<string, unknown>[] = [];
  const queue: object[] = [];
  let wake: (() => void) | null = null;
  let closed = false;
  const push = (messages: object[]) => {
    queue.push(...messages);
    wake?.();
  };
  const event = (result: object) => ({
    method: "chainHead_v1_followEvent",
    params: { subscription: "sub", result },
  });
  const connection: JsonRpcConnection = {
    send(request) {
      const message = JSON.parse(request) as Record<string, unknown>;
      sent.push(message);
      if (message.method === "chain_getBlockHash")
        push([{ id: message.id, result: node.genesis ?? PASEO_PEOPLE }]);
      if (message.method === "system_version")
        push([
          { id: message.id, result: node.version ?? "1.25.1-4a0cc2e7449" },
        ]);
      if (message.method === "rpc_methods")
        push([
          {
            id: message.id,
            result: {
              methods: node.methods ?? [
                "chainHead_v1_follow",
                "chainHead_v1_storage",
                "chain_getHeader",
                "chain_getBlockHash",
                "chain_getFinalizedHead",
                "state_getStorage",
                "statement_submit",
              ],
            },
          },
        ]);
      if (message.method === "chainHead_v1_follow")
        push([{ id: 1, result: "sub" }, ...events.map(event)]);
      if (message.method === "chainHead_v1_storage") {
        const [, block, items] = message.params as [
          string,
          string,
          { key: string }[],
        ];
        push([
          { id: message.id, result: { result: "started", operationId: "op" } },
          event({
            event: "operationStorageItems",
            operationId: "op",
            items:
              value === null || !holds(block)
                ? []
                : items.map(({ key }) => ({ key, value })),
          }),
          event({ event: "operationStorageDone", operationId: "op" }),
        ]);
      }
    },
    async *responses() {
      while (!closed) {
        const next = queue.shift();
        if (next === undefined) {
          await new Promise<void>((resolve) => (wake = resolve));
          continue;
        }
        yield JSON.stringify(next);
      }
    },
    close() {
      closed = true;
      wake?.();
    },
  };
  const readAt = () =>
    sent
      .filter((message) => message.method === "chainHead_v1_storage")
      .map((message) => (message.params as unknown[])[1]);
  return { connection, readAt, closed: () => closed };
}

const KEY = "0x3a73746174656d656e74" as HexString;
const FINAL = `0x${"f0".repeat(32)}`;
const CHILD = `0x${"c1".repeat(32)}`;
const SIBLING = `0x${"5b".repeat(32)}`;
const newBlock = (block: string, parent: string) => ({
  event: "newBlock",
  blockHash: block,
  parentBlockHash: parent,
});
const best = (block: string) => ({
  event: "bestBlockChanged",
  bestBlockHash: block,
});

describe("reading after a child of the claim's block", () => {
  test("a pruned claim ends the best read immediately", async () => {
    const chain = following(
      [
        { event: "initialized", finalizedBlockHashes: [FINAL] },
        {
          event: "finalized",
          finalizedBlockHashes: [SIBLING],
          prunedBlockHashes: [CLAIM_BLOCK],
        },
      ],
      null,
    );
    expect(
      await readAfterChild(chain.connection, KEY, CLAIM_BLOCK, 1_000),
    ).toEqual({ tag: "unseen" });
    expect(chain.closed()).toBe(true);
  });

  test("a real connection must report the pinned genesis, SDK version and required RPC methods before a best read", async () => {
    const events = [
      { event: "initialized", finalizedBlockHashes: [FINAL] },
      newBlock(CLAIM_BLOCK, FINAL),
      newBlock(CHILD, CLAIM_BLOCK),
      best(CHILD),
    ];
    for (const node of [
      { genesis: "0xwrong" },
      { version: "1.25.0-unknown" },
      { methods: ["statement_submit"] },
    ]) {
      const chain = following(events, "0x0100000000280000", () => true, node);
      await expect(
        readAfterChild(
          chain.connection,
          KEY,
          CLAIM_BLOCK,
          1_000,
          undefined,
          PASEO_PEOPLE,
        ),
      ).rejects.toThrow("not checked");
      expect(chain.readAt()).toEqual([]);
      expect(chain.closed()).toBe(true);
    }
    const chain = following(events, "0x0100000000280000");
    expect(
      await readAfterChild(
        chain.connection,
        KEY,
        CLAIM_BLOCK,
        1_000,
        undefined,
        PASEO_PEOPLE,
      ),
    ).toMatchObject({ tag: "child" });
  });
  test("reads at the first best block that descends from the claim's block, not at the claim's block itself", async () => {
    const chain = following(
      [
        { event: "initialized", finalizedBlockHashes: [FINAL] },
        newBlock(CLAIM_BLOCK, FINAL),
        best(CLAIM_BLOCK),
        newBlock(CHILD, CLAIM_BLOCK),
        best(CHILD),
      ],
      "0x0100000000280000",
    );
    expect(
      await readAfterChild(chain.connection, KEY, CLAIM_BLOCK, 1_000),
    ).toEqual({ tag: "child", block: CHILD, value: "0x0100000000280000" });
    expect([chain.readAt(), chain.closed()]).toEqual([[CHILD], true]);
  });

  test("answers final for a claim's block that is finalized already, and reads nothing", async () => {
    const chain = following(
      [{ event: "initialized", finalizedBlockHashes: [CLAIM_BLOCK, FINAL] }],
      null,
    );
    expect(
      await readAfterChild(chain.connection, KEY, CLAIM_BLOCK, 1_000),
    ).toEqual({ tag: "final" });
    expect(chain.readAt()).toEqual([]);
  });

  // A best chain that left the claim's block, or a node that never saw it.
  test("answers unseen when no best block descends from the claim's block in time", async () => {
    const chain = following(
      [
        { event: "initialized", finalizedBlockHashes: [FINAL] },
        newBlock(CLAIM_BLOCK, FINAL),
        newBlock(SIBLING, FINAL),
        best(SIBLING),
        newBlock(CHILD, SIBLING),
        best(CHILD),
      ],
      "0x0100000000280000",
    );
    expect(
      await readAfterChild(chain.connection, KEY, CLAIM_BLOCK, 50),
    ).toEqual({ tag: "unseen" });
    expect([chain.readAt(), chain.closed()]).toEqual([[], true]);
  });
});

describe("the People chain's guard", () => {
  // A chain where the claim's block has a child at the best block that holds
  // the allowance, and the finalized head does not hold it yet.
  function people() {
    const connections: ReturnType<typeof following>[] = [];
    const chain = {
      connect: async () => {
        const opened = following(
          [
            { event: "initialized", finalizedBlockHashes: [FINAL] },
            newBlock(CLAIM_BLOCK, FINAL),
            best(CLAIM_BLOCK),
            newBlock(CHILD, CLAIM_BLOCK),
            best(CHILD),
          ],
          "0x0100000000280000",
          (hash) => hash === CHILD,
        );
        connections.push(opened);
        return opened.connection;
      },
    };
    return {
      access: { chain, genesis: PASEO_PEOPLE as HexString },
      connections,
    };
  }

  test("a session that ends stops the check behind a post at once: no wait for the next read, nothing logged, every connection closed", async () => {
    const { access, connections } = people();
    const log: string[] = [];
    const guard = peopleAllowanceGuard({
      people: () => access,
      rule: () => BEST,
      log: (line) => log.push(line),
    });
    const session = new AbortController();
    const usable = await guard(CLAIM, { signal: session.signal });
    expect(usable.usableAt).toBe("best");
    // The check behind the post has read once, found nothing final, and sleeps 3 s.
    await new Promise((resolve) => setTimeout(resolve, 50));
    const started = Date.now();
    session.abort();
    expect(await usable.final).toBe(
      "the session ended before its allowance was final",
    );
    expect(Date.now() - started).toBeLessThan(1_000);
    expect(log).toHaveLength(1);
    expect(connections.every((opened) => opened.closed())).toBe(true);
  });
});
