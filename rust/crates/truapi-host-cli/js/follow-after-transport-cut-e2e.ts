// Product script for `scripts/follow-after-transport-cut-e2e.sh`, which
// describes the check.

const STORAGE_METHOD = "remote_chain_head_storage";
/** Marker for a follow stream that ended with no failure at all. */
const COMPLETED_WITHOUT_A_FAILURE = "completed without a failure";
// `System::Number`, present on every chain.
const SYSTEM_NUMBER =
  "0x26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";
const TIMEOUT_MS = 60_000;
/** Reads on the dead follow, spread wide enough to outlast a reconnect. */
const READS_AFTER_THE_CUT = 5;
const READ_GAP_MS = 2_000;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function within<T>(promise: PromiseLike<T>, what: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) =>
      setTimeout(
        () => reject(new Error(`timed out waiting for ${what}`)),
        TIMEOUT_MS,
      ),
    ),
  ]);
}

/** The `HostFailure`/`Unavailable` reason behind a call or subscription error. */
function reasonOf(error: unknown): string {
  const callError =
    error instanceof Error && "reason" in error ? error.reason : error;
  if (
    typeof callError === "object" &&
    callError !== null &&
    "value" in callError &&
    typeof callError.value === "object" &&
    callError.value !== null &&
    "reason" in callError.value &&
    typeof callError.value.reason === "string"
  ) {
    return callError.value.reason;
  }
  return `no reason on ${JSON.stringify(error)}`;
}

const bulletin = await truapi.chain.getChainInfo({ chain: "Bulletin" });
assert(bulletin.isOk(), "getChainInfo(Bulletin) failed:", bulletin);
const genesisHash = bulletin.value.genesisHash;

/** Opens a follow and resolves once it reports its first finalized block. */
function followBulletin(label: string) {
  let onInitialized: (hash: `0x${string}`) => void = () => {};
  const initialized = new Promise<`0x${string}`>((resolve) => {
    onInitialized = resolve;
  });
  const failures: string[] = [];
  const handle = truapi.chain
    .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    .subscribe({
      next: (item) => {
        if (item.tag !== "Initialized") return;
        const hash = item.value.finalizedBlockHashes.at(-1);
        assert(hash, `${label}: Initialized carried no finalized block`);
        onInitialized(hash);
      },
      error: (error) => failures.push(reasonOf(error)),
      complete: () => failures.push(COMPLETED_WITHOUT_A_FAILURE),
    });
  return { handle, initialized, failures };
}

const readNumber = (followSubscriptionId: string, hash: `0x${string}`) =>
  truapi.chain.getHeadStorage({
    genesisHash,
    followSubscriptionId,
    hash,
    items: [{ key: SYSTEM_NUMBER, queryType: "Value" }],
  });

// The product follows the chain and reads through that follow.
const dying = followBulletin("the first follow");
const hash = await within(dying.initialized, "the follow to initialize");
console.log(`follow ${dying.handle.subscriptionId} initialized at ${hash}`);

// The proxy closes the connection under this read rather than forwarding it.
const cut = await within(
  readNumber(dying.handle.subscriptionId, hash),
  "the read that cuts the connection",
);
assert(cut.isErr(), "the read succeeded; the proxy did not cut the connection");
console.log(`the read that lost the connection failed: ${reasonOf(cut.error)}`);

// The follow has to report the loss, or the product never learns to re-follow.
await within(
  (async () => {
    while (dying.failures.length === 0) await sleep(100);
  })(),
  "the follow to report the lost connection",
);
assert(
  dying.failures.length === 1,
  "the follow reported more than one ending:",
  dying.failures,
);
// A stream that just ends carries no reason, and a product cannot tell that
// from an orderly finish, so it would keep using a follow that is already gone.
assert(
  dying.failures[0] !== COMPLETED_WITHOUT_A_FAILURE,
  "the follow ended without reporting a failure, so nothing tells the product to re-follow",
);
console.log(`the follow ended with: ${dying.failures[0]}`);

// The core opens a replacement connection but never re-follows on it, so this
// follow is gone for good. Reads still bound to it have to keep failing, and
// keep saying which call failed; a read that hangs or quietly succeeds would
// leave the product believing a dead follow still works.
const reasons = new Set<string>();
for (let read = 1; read <= READS_AFTER_THE_CUT; read++) {
  await sleep(READ_GAP_MS);
  const again = await within(
    readNumber(dying.handle.subscriptionId, hash),
    `read ${read} on the dead follow`,
  );
  assert(again.isErr(), `read ${read} on the dead follow succeeded`);
  const reason = reasonOf(again.error);
  assert(
    reason.startsWith(`${STORAGE_METHOD}: `) &&
      reason.slice(STORAGE_METHOD.length + 2).trim().length > 0,
    `read ${read} does not say why it failed: "${reason}"`,
  );
  reasons.add(reason);
}
console.log(
  `${READS_AFTER_THE_CUT} reads on the dead follow all failed: ${[...reasons].join(" | ")}`,
);

// Nothing is wrong with the chain or the replacement connection: re-following
// is the whole recovery, and it works.
const live = followBulletin("the replacement follow");
const freshHash = await within(
  live.initialized,
  "the replacement follow to initialize",
);
const fresh = await within(
  readNumber(live.handle.subscriptionId, freshHash),
  "the read on the replacement follow",
);
assert(
  fresh.isOk(),
  "the replacement follow could not read:",
  fresh.isErr() ? reasonOf(fresh.error) : fresh,
);
console.log(
  `follow ${live.handle.subscriptionId} read over the replacement connection`,
);

export {};
