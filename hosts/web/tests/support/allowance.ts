import type { ClaimedAccount } from "../../src/platform/allowance-finality.js";
import type {
  AllowanceGuard,
  UsableAllowance,
} from "../../src/platform/allowance-usable.js";

/**
 * Stand-ins for the allowance guard (`src/platform/allowance-usable.ts`),
 * for tests about something else: what it answers for `claim`, usable at
 * `usableAt` with `final` behind it.
 */
export function usable(
  claim: ClaimedAccount,
  usableAt: "best" | "finalized" = "finalized",
  final: Promise<string | null> = Promise.resolve(null),
): UsableAllowance {
  return {
    account: claim.account,
    seq: claim.seq,
    collection: claim.collection,
    claimBlock: claim.blockHash,
    claimFinal: claim.blockHash !== undefined && usableAt === "finalized",
    running: () => true,
    usableAt,
    block: usableAt === "best" ? "0xbe57" : "0xf1a1",
    grant: { maxCount: 1, maxSize: 10_240 },
    waitedMs: 0,
    final,
  };
}

/** A guard that lets every claimed account post at once, at the finalized head. */
export const finalizedGuard: AllowanceGuard = async (claim) => usable(claim);
