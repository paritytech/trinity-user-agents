// Product script for `e2e/cli/chain-head-failure-cause.sh`, which describes
// the check.

const STORAGE_METHOD = "remote_chain_head_storage";
const FOLLOW_METHOD = "remote_chain_head_follow";
// `System::Number`, present on every chain.
const SYSTEM_NUMBER =
  "0x26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";
const TIMEOUT_MS = 60_000;

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

/** The `HostFailure` reason of a call error or of a follow's `SubscriptionError`. */
function reasonOf(error: unknown): string {
  const callError =
    error instanceof Error && "reason" in error ? error.reason : error;
  if (
    typeof callError === "object" &&
    callError !== null &&
    "tag" in callError &&
    callError.tag === "HostFailure" &&
    "value" in callError &&
    typeof callError.value === "object" &&
    callError.value !== null &&
    "reason" in callError.value &&
    typeof callError.value.reason === "string"
  ) {
    return callError.value.reason;
  }
  return `not a HostFailure: ${JSON.stringify(error)}`;
}

function namesItsCause(reason: string, method: string): boolean {
  return (
    reason.startsWith(`${method}: `) &&
    reason.slice(method.length + 2).trim().length > 0
  );
}

const bulletin = await truapi.chain.getChainInfo({ chain: "Bulletin" });
assert(bulletin.isOk(), "getChainInfo(Bulletin) failed:", bulletin);
const genesisHash = bulletin.value.genesisHash;

let followFailed: (error: unknown) => void = () => {};
const followFailure = new Promise<unknown>((resolve) => {
  followFailed = resolve;
});
let initialized: (hash: `0x${string}`) => void = () => {};
const finalized = new Promise<`0x${string}`>((resolve) => {
  initialized = resolve;
});

const follow = truapi.chain
  .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
  .subscribe({
    next: (item) => {
      if (item.tag === "Initialized") {
        const hash = item.value.finalizedBlockHashes.at(-1);
        assert(hash, "Initialized carried no finalized block");
        initialized(hash);
      }
    },
    error: (error) => followFailed(error),
    complete: () => followFailed("the follow completed without an error"),
  });

const hash = await within(finalized, "the follow to initialize");
console.log(`follow initialized at ${hash}`);

const storage = await within(
  truapi.chain.getHeadStorage({
    genesisHash,
    followSubscriptionId: follow.subscriptionId,
    hash,
    items: [{ key: SYSTEM_NUMBER, queryType: "Value" }],
  }),
  "the storage call to fail",
);
assert(
  storage.isErr(),
  "the storage call succeeded; the proxy did not cut the connection",
);
const storageReason = reasonOf(storage.error);
console.log(`storage call failed: ${storageReason}`);

const followReason = reasonOf(
  await within(followFailure, "the follow to fail"),
);
console.log(`follow failed: ${followReason}`);

assert(
  namesItsCause(storageReason, STORAGE_METHOD),
  `the storage failure does not say why it failed: "${storageReason}"`,
);
assert(
  namesItsCause(followReason, FOLLOW_METHOD),
  `the follow failure does not say why it failed: "${followReason}"`,
);
console.log("both failures name the closed connection as their cause");

export {};
