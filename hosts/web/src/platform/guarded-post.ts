import type { ClaimedAccount } from "./allowance-finality.js";
import type { AllowanceGuard, UsableAllowance } from "./allowance-usable.js";
import { decodeStatement, encodeSigned } from "./statement.js";
import { isStored, type SubmitResult } from "./statement-store.js";

/**
 * One statement the host posts itself, signed by an account the core claims
 * an allowance for: claim, wait until the allowance is usable
 * (`./allowance-usable.ts`), submit, and, when it was usable only at a best
 * block, follow the claim to finality behind the post.
 *
 * Ordinary retries send the same bytes: an
 * sr25519 signature differs each time it is made, so signing again would
 * make another statement. A node that answers `noAllowance` has not seen
 * the claim yet, so the bytes go again, {@link NO_ALLOWANCE_SUBMITS} times
 * at most. Nothing else is tried again here.
 *
 * When a best-block claim is not final within the finality wait, the post is
 * recovered once, only if the same account has a finalized allowance. No
 * recovery claim spends a slot after an ambiguous claim or reorg. A higher
 * expiry and new proof preserve the encrypted payload but escape peers'
 * evicted-hash bans (SDK 4a0cc2e7449). A node answers `known`
 * for a statement it holds, which does not send it to its peers again. The
 * outcome says what happened; it does not promise that anyone received the
 * statement.
 */

/** Submits of one statement while the node answers `noAllowance`, the first included. */
export const NO_ALLOWANCE_SUBMITS = 6;
/** Wait between two submits that the node answered `noAllowance`. */
export const NO_ALLOWANCE_INTERVAL_MS = 2_000;

/** Whether the node refused a statement because it does not see the account's allowance. */
export const isNoAllowance = (result: SubmitResult): boolean =>
  result.status === "rejected" && (result.reason ?? "").includes("noAllowance");

export interface GuardedPost {
  /**
   * Claim today's allowance of the account that signed {@link bytes}. Every
   * call claims for the same account; throws why not.
   */
  claim(): Promise<ClaimedAccount>;
  /** The statement, signed once. */
  bytes: Uint8Array;
  /** Sign a deliberate replacement after an unconfirmed best-block claim, never an arbitrary submit failure. */
  sign?(payload: Uint8Array): Uint8Array;
  /** Keep replacement bytes even if their submit answer is lost. */
  replaced?(bytes: Uint8Array): void;
  /** Resume against finalized allowance; `claim` must return the cached answer, not submit another claim. */
  retryUnconfirmed?: boolean;
  submit(bytes: Uint8Array): Promise<SubmitResult>;
  /** Why the post is no longer wanted, or null. Asked before each step. */
  unwanted(): string | null;
}

export interface GuardedPostDeps {
  guard: AllowanceGuard;
  sleep(ms: number): Promise<void>;
  now(): number;
}

/** How a post that went at a best block ended, once its claim's finality was known. */
export type FinalOutcome =
  /** The allowance held at the finalized head, or the post waited for that before it went. */
  | { tag: "final" }
  /** The same account held at the finalized head and a higher-priority replacement went. */
  | { tag: "reposted"; why: string; result: SubmitResult }
  /** It was not final, and the one recovery did not post it again. */
  | { tag: "unconfirmed"; why: string };

export interface PostedStatement {
  /** The store's last answer. */
  result: SubmitResult;
  /** Submits made, the `noAllowance` ones included. */
  submits: number;
  usable: UsableAllowance;
  /** Settles once the post stands on a finalized allowance, or not. Never rejects. */
  final: Promise<FinalOutcome>;
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/** Submit `post`'s bytes, again while the node answers `noAllowance`. */
export async function submitUntilAllowed(
  post: Pick<GuardedPost, "bytes" | "submit" | "unwanted">,
  sleep: (ms: number) => Promise<void>,
): Promise<{ result: SubmitResult; submits: number }> {
  let submits = 0;
  for (;;) {
    const why = post.unwanted();
    if (why !== null) throw new Error(why);
    const result = await post.submit(post.bytes);
    submits += 1;
    if (!isNoAllowance(result) || submits >= NO_ALLOWANCE_SUBMITS)
      return { result, submits };
    await sleep(NO_ALLOWANCE_INTERVAL_MS);
  }
}

/** Claim, wait until usable, and submit `post`; see the module. Throws why it did not reach the store. */
export async function postWhenUsable(
  post: GuardedPost,
  deps: GuardedPostDeps,
): Promise<PostedStatement> {
  const check = () => {
    const why = post.unwanted();
    if (why !== null) throw new Error(why);
  };
  check();
  const claim = await post.claim();
  check();
  const usable = await deps.guard(
    claim,
    post.retryUnconfirmed ? { wait: "finalized" } : undefined,
  );
  check();
  if (!usable.running()) throw new Error("the session ended");
  let bytes = post.bytes;
  if (post.retryUnconfirmed) {
    if (Math.floor(deps.now() / 86_400_000) !== claim.period)
      throw new Error("the day changed");
    const original = decodeStatement(bytes);
    if (deps.now() >= Number(original.expiry >> 32n) * 1000)
      throw new Error("the statement has expired");
    if (post.sign !== undefined) {
      bytes = encodeSigned(
        { ...original, expiry: original.expiry + 1n },
        original.signer,
        post.sign,
      );
      post.replaced?.(bytes);
    }
  }
  const sent = await submitUntilAllowed({ ...post, bytes }, deps.sleep);
  const final =
    usable.usableAt === "finalized" || !isStored(sent.result)
      ? Promise.resolve<FinalOutcome>({ tag: "final" })
      : usable.final.then((why): FinalOutcome | Promise<FinalOutcome> =>
          why === null
            ? { tag: "final" }
            : !usable.running()
              ? { tag: "unconfirmed", why: "the session ended" }
              : recover(post, deps, claim, why, usable.running),
        );
  return { ...sent, usable, final };
}

/** The one recovery of a best-block post whose claim did not become final. */
async function recover(
  post: GuardedPost,
  deps: GuardedPostDeps,
  first: ClaimedAccount,
  why: string,
  running: () => boolean,
): Promise<FinalOutcome> {
  const unconfirmed = (reason: string): FinalOutcome => ({
    tag: "unconfirmed",
    why: `${why}; ${reason}`,
  });
  try {
    const original = decodeStatement(post.bytes);
    const expiresMs = Number(original.expiry >> 32n) * 1000;
    const check = () => {
      if (!running()) throw new Error("the session ended");
      const unwanted = post.unwanted();
      if (unwanted !== null) throw new Error(unwanted);
      if (deps.now() >= expiresMs) throw new Error("the statement has expired");
      if (Math.floor(deps.now() / 86_400_000) !== first.period)
        throw new Error("the day changed, so it was not posted again");
    };
    check();
    if (post.sign === undefined)
      throw new Error("no replacement signer; retry explicitly");
    await deps.guard({ ...first, blockHash: undefined }, { wait: "finalized" });
    check();
    const bytes = encodeSigned(
      { ...original, expiry: original.expiry + 1n },
      original.signer,
      post.sign,
    );
    post.replaced?.(bytes);
    const sent = await submitUntilAllowed(
      {
        ...post,
        bytes,
        unwanted: () => {
          try {
            check();
            return null;
          } catch (error) {
            return errorText(error);
          }
        },
      },
      deps.sleep,
    );
    return isStored(sent.result)
      ? { tag: "reposted", why, result: sent.result }
      : unconfirmed(
          `posted again once final, the Statement Store answered ${sent.result.status}${sent.result.reason === undefined ? "" : ` (${sent.result.reason})`}`,
        );
  } catch (error) {
    return unconfirmed(errorText(error));
  }
}
