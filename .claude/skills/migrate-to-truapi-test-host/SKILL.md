---
name: migrate-to-truapi-test-host
description: Migrate a product's Playwright e2e suite from @parity/host-api-test-sdk to @parity/truapi-host/testing. Use when a repo's e2e fixtures import host-api-test-sdk, or when asked to adopt the TrUAPI test host.
---

# Migrating a suite onto the TrUAPI test host

`@parity/host-api-test-sdk` reimplements the protocol in TypeScript.
`@parity/truapi-host/testing` runs the real Rust core and mocks only the
platform seam, so a suite that migrates starts testing what ships.

The method names on the fixture deliberately match `TestHostAPI`, so most specs
are untouched. Three assertion patterns do change, and one of them is a real
behavioural difference rather than churn. Work through the steps in order.

## 0. Preconditions — check all five before changing anything

A migration that starts before these hold produces failures that look like
fixture bugs and are not. Check them in order and stop at the first that fails.

**1. The product resolves `@parity/truapi` 0.16.0 or later.**

```bash
node -e "console.log(require('@parity/truapi/package.json').version)"
```

Below 0.16.0 the product speaks wire codec 1 and the handshake refuses: every
host call goes unanswered, the app parks at its connecting state, and every
locator times out. `@parity/product-sdk-host` pins `^0.16.0` only from 0.20.0
(shipped in `@parity/product-sdk` 0.28.0); 0.19.1 still pins `^0.13.1`. If the
version is below that, the work is a product-sdk bump and this migration cannot
be started yet.

**2. The product uses `@parity/product-sdk`, not `@novasamatech/host-api`.**

```bash
node -e "try{console.log(require('@novasamatech/host-api/package.json').version)}catch{console.log('absent — good')}"
```

A repo still on the `@novasamatech` line is a client-SDK generation behind.
Adopting `@parity/product-sdk` comes first; swapping the test host means
nothing until then.

**3. The suite passes today.** Record the numbers before touching anything:

```bash
npx playwright test --reporter=line   # or the repo's own e2e command
```

Migrating a red suite makes it impossible to tell your changes from its
existing failures, and a suite that cannot even load (a stale import, a deleted
export) will look like the new fixture rejecting it.

**4. Write tests have a funded account.** TrUAPI derives a product account from
(session root, product id) — it cannot be assigned. A suite that submits
transactions signs with the DERIVED address, so that address needs funding.
Read it back from the host and fund it; do not assume a funder seed signs.
Until this is settled a write test HANGS at `signSubmitAndWatch` rather than
failing, which is the single most expensive failure mode in this migration.

**5. No spec depends on a refused capability.** Grep first:

```bash
grep -rEn "getPaymentLog|clearPaymentLog|setPaymentBalance|setPaymentTopUpBehavior|simulatePaymentStatus|setLoginBehavior" e2e/
```

Any hit is a spec that needs rewriting or skipping, not porting — see section 4.
`setLoginBehavior` is the sharpest: suites pass `"success"`/`"reject"` to drive
an RFC-0009 login flow, and the fixture option only takes `"auto" | "manual"`,
because this is a signing host and that flow belongs to a pairing host.

The statement-store controls and `injectChatAction` are served, so they are not
in this grep. `injectStatement` takes `{topics, data}` the way the old package
spells it, or the SCALE wire bytes, and answers the `StatementEntry` it
retained; `getSubmittedStatements` and `getStatements` answer entries too, so a
suite reading `0x` hex off them reads `entry.data` instead.
`injectChatAction` takes the core's `HostChatActionSubscribeItem` rather than
`{roomId, peer, payload}`, so that call site changes shape even though it keeps
working.

## 1. Swap the import

```diff
-import { createTestHostFixture, type TestHost } from "@parity/host-api-test-sdk/playwright";
+import { createTestHostFixture, type TestHost } from "@parity/truapi-host/testing/playwright";
```

Network constants and the log-entry types come from the same subpath, so a file
importing `PASEO_ASSET_HUB`, `SigningLogEntry`, `PermissionLogEntry`,
`ChatMessageLogEntry`, `LoginBehavior` or `DevAccountName` needs no second
import path.

`e2e/helpers.ts` usually has one `import type { TestHost }` to change too.
`HexString` is on that subpath as well, for a file annotating a `genesisHash`.

Swap the dependency, and add `esbuild`: the package takes it as a peer, so a
suite without it fails to resolve at import rather than at run.

```diff
-"@parity/host-api-test-sdk": "catalog:",
+"@parity/truapi-host": "catalog:",
+"esbuild": "^0.25.0",
```

## 2. Delete the server plumbing

The fixture starts and shares its own host server when none is given, so
`productUrl` is the only required option.

```diff
-import { createTestHostServer } from "@parity/host-api-test-sdk";
-
-const server = await createTestHostServer();
-
 export const test = base.extend<{ testHost: TestHost }>(
     createTestHostFixture({
         productUrl: PRODUCT_URL,
-        hostUrl: server.url,
         accounts: ["alice", "bob"],
     }),
 );
```

Chain options need no edit. Both spellings are accepted -- `networks: [X]` and
the older `chain: X`, which is exactly `networks: [X]` -- and either expands
into the chain proxy, the reported chain set and the runtime genesis, which
have to agree. Passing both is refused rather than merged.

A suite that starts the server itself also needs no restructuring:
`createTestHostServer` takes the same host configuration it always did
(`productUrl`, `accounts`, `networks`) and returns a URL ready to open.

Set `productId` to the dotNS identifier the product signs with. A product that
derives its own identifier from `window.location.host` wants that value here --
under Playwright that is the dev-server origin, e.g. `"localhost:5199"`.

Get it wrong and every signing call fails with `PermissionDenied`, which reads
like a permission-policy problem and is not one: the core scopes a product
account to its identifier and refuses a request scoped to a different one. If
signing fails but reads pass, set this before looking anywhere else.

### Allowances are granted without being performed, by default

`allowances` defaults to `"granted"`: every allocation is answered as allocated
without being carried out, and the statement store is served in-page. A suite
that allocates allowances, signs with the key it gets back and submits
statements therefore passes with no on-chain identity.

The store is served on the People chain when the suite declares one, and on the
single declared chain when it does not -- that proxy takes every request, so it
carries the statement traffic too. Declaring several chains with no People among
them is refused rather than served: there is no request the store could attach
to, and answering statement reads on a hub would answer what a hub refuses.

What that buys is narrower than it looks. Nothing is registered anywhere, so a
green run says the product handles a grant, not that a host would have given
one. A real statement store refuses what this one accepts.

Set `allowances: "chain"` for the real path -- ring membership, slot selection,
ring-VRF proof, extrinsic -- which is what a host actually does and what fails
where a host would:

```ts
createTestHostFixture({ productUrl: PRODUCT_URL, allowances: "chain" });
```

That path needs an account enrolled in a personhood ring on the target People
chain. A dev account is derived from fixed entropy and is enrolled in none, so
every allocation stops with:

```
WARN signing_host: direct resource allocation item failed
  {reason=signing account is not a personhood ring member;
   cannot grant statement-store allowance}
```

Pass an enrolled identity as explicit entropy to get past that:

```ts
accounts: [{ name: "enrolled", entropy: entropyFromMnemonic(PHRASE) }]
```

`truapi-host alloc-check --mnemonic "<phrase>"` reports whether an identity is a
ring member, read-only and in seconds. Run it before a browser suite: it answers
in one line what the suite takes twenty minutes to say less clearly.

### A suite that allocates allowances on-chain must serve the People chain

Only with `allowances: "chain"`. Allowance registration reads personhood
collections from the People chain, not the hub. Serving only the hub fails every
allocation with an outcome the product renders as "resource is unavailable",
and the core's log names the real cause:

```
WARN statement_allowance: could not resolve this collection
  {collection=People, err=Members.Collections[People] missing}
```

Pass both, using an `id` ending in `-people` to select the role:

```ts
networks: [PASEO, { id: "paseo-people", name: "Paseo People",
                    genesisHash: PEOPLE_GENESIS, rpcUrl: PEOPLE_WS }],
```

## 3. The three assertions that change

**Internal storage keys.** A spec reading the old host's private key layout
(`localStorage.getItem("test-host:demo:mykey")`) is coupled to one host's
implementation. Read through the control surface instead:

```diff
-const raw = await page.evaluate(() => localStorage.getItem("test-host:demo:mykey"));
+const raw = await testHost.findProductStorage("mykey");
```

`findProductStorage` reads the product's own key out of the namespaced one, so
the caller does not need to know the namespacing and a prefixed store stays
distinct from an unprefixed one. It returns `Uint8Array | undefined`;
`getProductStorageValue` returns the same value decoded as UTF-8.

**A pinned product account.** `productAccounts: { "demo.dot/0": "bob" }` is
rejected at construction, and cannot be supported: TrUAPI *derives* a product
account from (session root, product id), so it is not assignable. The old
package could pin one only because no core stood behind it.

Rewrite the assertion to read the address back rather than assert a known one:

```diff
-expect(await accountAddress()).toBe(BOB_ADDRESS);
+const derived = await accountAddress();
+expect(derived).toMatch(/^1/);           // SS58 prefix 0
+// and, if the test needs stability across a switch:
+await testHost.switchAccount("charlie");
+expect(await accountAddress()).not.toBe(derived);
+await testHost.switchAccount("bob");
+expect(await accountAddress()).toBe(derived);  // derivation is deterministic
```

Switching the host account **changes** the product account. Any spec asserting
it stays fixed is asserting the old package's behaviour, not the protocol's.

Addresses also differ from polkadot-js `//Alice` and friends: a session
activates from 32 bytes of entropy, not a derivation path. Never hard-code a
well-known address; read it back.

**A permission revoked mid-run.** The core persists a decided authorization per
(product, permission) and answers from its own storage, so a product asking
twice is answered from that record and the host is not called again. Setting the
answer on the mock is the suite talking to the host, not the product asking
again: it retracts that record, so the next request is put to the host afresh.

```ts
await client.permissions.requestDevicePermission("Camera"); // asks the host
await client.permissions.requestDevicePermission("Camera"); // answered from the record

testHost.revokePermission("Camera");
await client.permissions.requestDevicePermission("Camera"); // asks again, denied
```

A spec that revokes and then waits for a host-side entry works. One that expects
a *product* asking twice to re-prompt does not, and is asserting the old
package's behaviour.

### A suite that submits extrinsics must fund the product account first

A product account is derived, so it starts with no balance and nothing has ever
funded it. A suite that submits a real extrinsic fails with
`Invalid.Payment` -- before inclusion, with the rest of the protocol path
working, which reads like a host bug and is not one. The old package sidestepped
this by pinning the product to an already-funded dev key.

Work the address out before the run, not from the failures. This needs no host
and no browser:

```ts
import { productAccountAddress } from "@parity/truapi-host/testing/playwright";

const address = await productAccountAddress({
  account: "bob",            // the dev account the session activates from
  productId: "tx-demo.dot",  // index defaults to 0
});
```

Fund that address once, from a faucet or a transfer. Derivation is
deterministic, so it holds for every later run. Re-fund only when the suite
changes its `accounts` or its product id: both feed the derivation, so either
change is a different account.

That funded address is now the suite's to keep alive, and this is the part worth
writing down. `@parity/host-api-test-sdk` pinned the product to a dev account
such as Bob, and a testnet funds Bob as a matter of course, so no suite ever had
to think about a balance. A derived product account is specific to this suite
and nobody tops it up. It drains as the suite spends, and a chain reset empties
it outright -- and either way the next run fails as `Invalid.Payment` before
inclusion, the same opaque symptom as never having funded it.

Two things make that cheap to live with. Record the derived address somewhere a
person will find it, next to the suite rather than in a run log, so topping it
up does not start with re-deriving it. And check the balance in the run's setup
rather than letting the first extrinsic discover it: a setup that fails with the
address and the balance it found turns a protocol-looking failure into an
errand. A setup that funds the account itself removes the chore entirely, which
is worth doing once a suite runs anywhere unattended.

The address is encoded at the prefix the core mandates, which is **not**
necessarily the one the product displays -- a product rendering at another
prefix shows a different string for the same account. Pass this to a faucet or a
transfer as it stands, and compare against a product's own rendering by decoding
both.

Inside a running suite the same address is on the fixture, where it also tracks
`switchAccount`:

```ts
const address = await testHost.getProductAccountAddress();
```

### Withholding one resource while the rest stay granted

With allocation granted by default, a product's refusal path is unreachable.
Name the resources the host should refuse:

```ts
createTestHostFixture({
  // ...
  behaviors: { resourceAllocation: { AutoSigning: false } },
});
```

Unlisted resources stay granted. This is a boot option because a product asks
for its resources on connect, so a later call would land after that.

## 4. What refuses, and why

Six of `TestHostAPI`'s members throw with an explanation rather than returning
something plausible. The rest, including the statement-store controls and
`injectChatAction`, are served.

- payments (`getPaymentLog`, `setPaymentBalance`, `simulatePaymentStatus`,
  `setPaymentTopUpBehavior`, `clearPaymentLog`) -- the protocol declares them
  and no host implements them. Every method of the core's payment capability
  returns an error and ignores its arguments, so there is no behaviour to mock.
  A suite with payment specs cannot migrate them; it can only drop or rewrite
  them.
- `setLoginBehavior` -- fixed at boot. Pass `loginBehavior: "auto" | "manual"`
  to the fixture instead, which is where this host takes it.

A test reaching one of these gets a reason, not `is not a function`.

## 5. Verify

Run the suite and compare against its pre-migration result, not against green:

```bash
npx playwright test --reporter=line
```

Expect the same passes, minus anything in section 4. If a spec fails on a
locator timeout at the app's connecting state, re-check step 0 -- that is the
codec symptom, not a fixture problem.

### When a call fails and the product only says "error"

The core logs why a call failed before mapping it to a protocol answer, but it
runs in a Web Worker by default, and `page.on("console")` does not observe a
worker's console. Move it onto the page and raise the level:

```ts
createTestHostFixture({
  productUrl: PRODUCT_URL,
  topology: "main-thread",
  logLevel: "debug",
});
```

The reason then arrives on the page console:

```
WARN truapi::runtime::signing_host: direct resource allocation item failed
  {reason=signing host: statement-store allowance allocation is native-only}
```

Both are debugging aids. `"worker"` is the production topology, so take them
back out once the failure is understood.
