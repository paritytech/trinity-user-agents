import type { TrUApiClient } from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import type { Heard, Post } from "./protocol.js";

/**
 * DEV ONLY. How Contact Chat's posts travel: statements in the People
 * chain's Statement Store, through the product's own TrUAPI
 * `statementStore`, signed with `createProof` as the product's own account
 * at a conversation's raw derivation index. The core holds that account's
 * key; the product holds only the index.
 *
 * Through the host's light client a subscription gets no stored replay: its
 * first page is empty, and a post made before this side started reading is
 * read only if a peer sends it later. A full node would replay what it
 * holds first.
 */

/** How long a post stays in the store after it was made: two days. */
export const RETENTION_SECONDS = 2 * 86_400;

/** How long a subscription that ended waits before it starts again. */
export const FOLLOW_RETRY_MS = 15_000;

/**
 * A post's expiry: {@link RETENTION_SECONDS} from now in the high 32 bits, as
 * the store reads it, and above `previous`, since the store replaces a
 * statement on the same channel of the same account only with a strictly
 * higher expiry.
 */
export function messageExpiry(previous: bigint | null, nowMs: number): bigint {
  const expiry = BigInt(Math.floor(nowMs / 1000) + RETENTION_SECONDS) << 32n;
  return previous !== null && previous >= expiry ? previous + 1n : expiry;
}

export interface Statements {
  /** The product's own account at raw derivation index `index`, `0x` hex. */
  account(index: string): Promise<string>;
  /**
   * Follow `topics`: each statement on any of them that the subscription
   * delivers, until the returned function is called. A subscription that
   * ends starts again after {@link FOLLOW_RETRY_MS}; `problem` is told why it
   * ended.
   */
  follow(
    topics: string[],
    deliver: (heard: Heard[]) => void,
    problem: (reason: string) => void,
  ): () => void;
  /** Sign `post` as the account at `index`, to expire at `expiry`, and submit it. */
  post(index: string, post: Post, expiry: bigint): Promise<void>;
}

type StatementClient = Pick<TrUApiClient, "account" | "statementStore">;

/**
 * Why a TrUAPI call failed, as text the user may see: the reason its error
 * carries, else its kind. Never the error's raw value.
 */
function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  let value = error;
  for (let depth = 0; depth < 3; depth++) {
    if (typeof value !== "object" || value === null) break;
    const { reason, value: inner } = value as {
      reason?: unknown;
      value?: unknown;
    };
    if (typeof reason === "string") return reason;
    value = inner;
  }
  const kind = (error as { tag?: unknown } | null)?.tag;
  return typeof kind === "string"
    ? `the host answered ${kind}`
    : "unknown error";
}

/** The product's TrUAPI Statement Store, as Contact Chat uses it, for the product `productId`. */
export function truapiStatements(
  client: StatementClient,
  productId: string,
  schedule: (task: () => void, delayMs: number) => () => void,
): Statements {
  const accountAt = (index: string) => ({
    dotNsIdentifier: productId,
    derivationIndex: { tag: "Raw" as const, value: index as HexString },
  });
  return {
    async account(index) {
      const result = await client.account.getAccount({
        productAccountId: accountAt(index),
      });
      if (result.isErr()) throw new Error(errorText(result.error));
      return result.value.account.publicKey.toLowerCase();
    },

    follow(topics, deliver, problem) {
      let stopped = false;
      let current: { unsubscribe(): void } | null = null;
      let cancelRetry = () => {};
      const again = (reason: string) => {
        current = null;
        if (stopped) return;
        problem(reason);
        cancelRetry = schedule(start, FOLLOW_RETRY_MS);
      };
      function start(): void {
        if (stopped) return;
        current = client.statementStore
          .subscribe({
            request: { tag: "MatchAny", value: topics as HexString[] },
          })
          .subscribe({
            next: (page) =>
              deliver(
                page.statements.flatMap((statement) =>
                  statement.proof.tag === "Sr25519"
                    ? [
                        {
                          signer: statement.proof.value.signer.toLowerCase(),
                          topics: statement.topics.map((topic) =>
                            topic.toLowerCase(),
                          ),
                          data: (statement.data ?? "0x").toLowerCase(),
                        },
                      ]
                    : [],
                ),
              ),
            error: (error) => again(errorText(error)),
            complete: () => again("the subscription ended"),
          });
      }
      start();
      return () => {
        stopped = true;
        cancelRetry();
        current?.unsubscribe();
      };
    },

    async post(index, post, expiry) {
      const statement = {
        expiry,
        channel: post.channel as HexString,
        topics: post.topics as HexString[],
        data: post.data as HexString,
      };
      const proof = await client.statementStore.createProof({
        productAccountId: accountAt(index),
        statement,
      });
      if (proof.isErr()) throw new Error(errorText(proof.error));
      const result = await client.statementStore.submit({
        ...statement,
        proof: proof.value.proof,
      });
      if (result.isErr()) throw new Error(errorText(result.error));
    },
  };
}
