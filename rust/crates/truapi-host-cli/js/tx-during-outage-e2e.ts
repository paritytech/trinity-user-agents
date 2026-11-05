// Product script for `scripts/tx-during-outage-e2e.sh`, which describes the
// check.

// A storage key no chain has; reading it is the proxy's signal to cut.
const TRIGGER_KEY = `0x${"7a".repeat(32)}`;
const TIMEOUT_MS = 60_000;
/** Transactions requested while the connection is down, spread over the outage. */
const DURING_THE_OUTAGE = 3;
const REQUEST_GAP_MS = 500;

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

const describe = (error: unknown) =>
  error instanceof Error ? error.message : JSON.stringify(error);

const people = await truapi.chain.getChainInfo({ chain: "People" });
assert(people.isOk(), "getChainInfo(People) failed:", people);
const genesisHash = people.value.genesisHash;

const signer = {
  dotNsIdentifier: "headless-playground.dot",
  derivationIndex: { tag: "Index" as const, value: 0 },
};
// `System::remark` with an empty payload; nothing is submitted.
const createTransaction = () =>
  truapi.signing.createTransaction({
    signer,
    genesisHash,
    callData: "0x000000",
    extensions: [],
    txExtVersion: 0,
    contacts: [],
  });

// With the connection up, a transaction builds.
const before = await within(createTransaction(), "the first transaction");
assert(
  before.isOk(),
  "the first transaction failed with the connection up:",
  before.isErr() ? describe(before.error) : before,
);
console.log("a transaction built with the connection up");

// Reading the trigger key makes the proxy drop every connection and refuse
// reconnects for the outage window.
let onInitialized: (hash: `0x${string}`) => void = () => {};
const initialized = new Promise<`0x${string}`>((resolve) => {
  onInitialized = resolve;
});
const follow = truapi.chain
  .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
  .subscribe({
    next: (item) => {
      if (item.tag !== "Initialized") return;
      const hash = item.value.finalizedBlockHashes.at(-1);
      if (hash) onInitialized(hash);
    },
    error: () => {},
    complete: () => {},
  });
const hash = await within(initialized, "a follow to initialize");
await truapi.chain.getHeadStorage({
  genesisHash,
  followSubscriptionId: follow.subscriptionId,
  hash,
  items: [{ key: TRIGGER_KEY, queryType: "Value" }],
});
console.log("asked the proxy to cut");

// A product that asks for a transaction while its phone has no network should
// get one once the network is back, not a failure it has to retry itself.
const failures: string[] = [];
const requests = [];
for (let request = 1; request <= DURING_THE_OUTAGE; request++) {
  requests.push(
    within(createTransaction(), `transaction ${request}`).then((result) => {
      if (result.isErr()) failures.push(`transaction ${request}: ${describe(result.error)}`);
      else console.log(`transaction ${request} built`);
    }),
  );
  await sleep(REQUEST_GAP_MS);
}
await Promise.all(requests);
assert(
  failures.length === 0,
  `${failures.length} of ${DURING_THE_OUTAGE} transactions requested during the outage failed:\n${failures.join("\n")}`,
);

export {};
