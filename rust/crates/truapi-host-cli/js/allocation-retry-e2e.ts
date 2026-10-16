// A resource allocation retried after the product got no answer to the first
// one, against a pairing-host CLI paired with a real signing-host CLI.
//
// `scripts/allocation-retry-e2e.sh` runs the signing host, standing in for the
// phone, so it approves each allocation after the delays in
// `TRUAPI_ALLOCATION_APPROVAL_DELAYS_MS`:
//
//   1. baseline  approved at once, so the person holds a slot for this product
//   2. late      approved after the client's 120 s request deadline
//   3. retry     approved at once
//
// Reading the result:
// - Step 2 ends in `RequestTimeoutError`: the product stops waiting before the
//   person answers.
// - Step 3 answers `Allocated`, and the signing host logs `statement-store
//   allowance already allocated` and registers nothing on chain. A
//   registration means the retry asked for one more slot.
import { existsSync, readFileSync } from "node:fs";

const STATEMENT_STORE = { tag: "StatementStoreAllowance" } as const;
const REGISTERED = "registered statement-store allowance";
const REUSED = "statement-store allowance already allocated";
// Longer than the late step's approval delay, so a signing host that missed
// the withdrawal has approved it before the retry is counted.
const LATE_SETTLE_MS = 20_000;

const login = await truapi.account.requestLogin({ reason: undefined });
assert(
  login.isOk() && ["Success", "AlreadyConnected"].includes(String(login.value)),
  `pairing failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`,
);

const signerLog = process.env.E2E_SIGNER_LOG;
assert(
  signerLog,
  "E2E_SIGNER_LOG is not set; run scripts/allocation-retry-e2e.sh",
);

/** How often the signing host has logged `line` so far. */
const signerCount = (line: string) =>
  existsSync(signerLog)
    ? readFileSync(signerLog, "utf8").split(line).length - 1
    : 0;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const failures: string[] = [];
const check = (ok: boolean, failure: string) => {
  if (!ok) failures.push(failure);
};

/** Allocate a statement-store slot and describe how the call ended. */
async function allocate(): Promise<string> {
  const started = performance.now();
  try {
    const result = await truapi.resourceAllocation.request({
      resources: [STATEMENT_STORE],
    });
    const seconds = ((performance.now() - started) / 1000).toFixed(1);
    const outcome = result.isOk()
      ? result.value.outcomes.join(",")
      : `Err(${JSON.stringify(result.error)})`;
    console.log(`  -> ${outcome} after ${seconds}s`);
    return outcome;
  } catch (error) {
    const seconds = ((performance.now() - started) / 1000).toFixed(1);
    // Matched by name: the runner's client is a separate module instance.
    const outcome =
      error instanceof Error && error.name === "RequestTimeoutError"
        ? "RequestTimeoutError"
        : `threw ${error}`;
    console.log(`  -> ${outcome} after ${seconds}s`);
    return outcome;
  }
}

console.log("step 1: baseline allocation, approved at once");
const baseline = await allocate();
check(
  baseline === "Allocated",
  `the baseline allocation must succeed, got ${baseline}`,
);

console.log("step 2: the person approves after the 120 s request deadline");
const late = await allocate();
check(
  late === "RequestTimeoutError",
  `the product must stop waiting at its deadline, got ${late}`,
);
await sleep(LATE_SETTLE_MS);

console.log("step 3: the product retries");
const registeredBefore = signerCount(REGISTERED);
const reusedBefore = signerCount(REUSED);
const retry = await allocate();
// The signing host logs before it answers, and the log is piped through tee.
await sleep(1_000);
const registered = signerCount(REGISTERED) - registeredBefore;
const reused = signerCount(REUSED) - reusedBefore;
console.log(
  `  signing host: ${registered} slot(s) registered, ${reused} reused`,
);
check(retry === "Allocated", `the retry must succeed, got ${retry}`);
check(
  registered === 0 && reused === 1,
  `the retry must reuse the person's slot: ${registered} registered, ${reused} reused`,
);

assert(
  failures.length === 0,
  `allocation retry e2e failed:\n- ${failures.join("\n- ")}`,
);
console.log("the retry after an unanswered allocation reused its slot");
