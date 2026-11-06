---
name: migrate-to-truapi-test-host
description: Migrate a product's Playwright e2e suite from @parity/host-api-test-sdk to @parity/truapi-host/testing. Use when a repo's e2e fixtures import host-api-test-sdk, or when asked to adopt the TrUAPI test host.
---

# Migrating a suite onto the TrUAPI test host

`@parity/host-api-test-sdk` reimplements the protocol in TypeScript.
`@parity/truapi-host/testing` runs the real Rust core and mocks only the
platform seam, so a suite that migrates starts testing what ships.

The method names on the fixture deliberately match `TestHostAPI`, so most specs
are untouched. A few assertion patterns do change, and some are real
behavioural differences rather than churn. Work through the steps in order.

A suite that reads no chain, submits nothing and uses no refused call can skip
the allowance, funding, withholding and several-signer parts. It still needs
sections 0 and 1, all of section 2 except allowances, section 3 up to funding
(storage, accounts, usernames, permissions and the smaller changes), and
section 5.

## 0. Preconditions — check all five before changing anything

A migration that starts before these hold produces failures that look like
fixture bugs and are not. Check them in order and stop at the first that fails.

**1. The product resolves `@parity/truapi` 0.24.0 or later.**

```bash
pnpm why @parity/truapi   # or: npm ls @parity/truapi
```

Read the copy resolved by the product SDK the product depends on directly.
Others can appear under the old test SDK or a tool with its own product SDK.
`@parity/truapi` does not export its `package.json`, so a
`require('@parity/truapi/package.json')` check throws rather than answering.

0.13.1 and older speak wire codec 1: the product reports itself connected, but
no call is ever answered, every locator times out, and the host logs
`invalid frame: frame is missing the message-type byte`. 0.16.0 and 0.17.0
speak codec 2, and 0.18.0 onwards codec 3, the only codec the core is built
for. A codec-2 product answered the basic calls in testing (account, username,
permission, storage, signing, allocation, statements, chain reads), but the
subscriptions whose start frames changed in codec 3, such as theme, locale and
connection status, were not tested.

The floor is set by the test host, not the codec. `@parity/truapi-host` first
ships `./testing` in 0.19, and `productAccountAddress` and
`getProductStorageValue`, which this skill relies on, first appear in 0.24. A
product should pair with the same minor, so it needs `@parity/truapi` 0.24.0 or
later, which `@parity/product-sdk-host` brings from 0.25.0. Below that, the
work is a product-sdk bump and this migration cannot be started yet.

Use the `@parity/truapi-host` release with the same minor as the product's
`@parity/truapi`, since that is the pair each release is tested as. If the repo
pins `@parity/truapi` in `overrides`, move it with
`truapi-host`: bumping the host alone leaves its JS on the old truapi while its
WASM core moves.

**2. The product uses `@parity/product-sdk`, not `@novasamatech/host-api`.**

```bash
grep -rn "@novasamatech/host-api" src/ package.json
```

What matters is the product's own source and direct dependencies. A copy that
another package pulls in, such as a workspace package or a deploy tool, does
not block the migration, so `npm ls @novasamatech/host-api` can list it without
the product being on that line.

A repo still on the `@novasamatech` line is a client-SDK generation behind.
Adopting `@parity/product-sdk` comes first; swapping the test host means
nothing until then.

**3. The suite passes today.** Record the numbers before touching anything,
using the repo's own e2e command:

```bash
npm run test:e2e                                  # the repo's script, if it has one
npx playwright test -c <e2e dir> --reporter=line  # otherwise, pointing at the config
```

Migrating a red suite makes it impossible to tell your changes from its
existing failures, and a suite that cannot even load (a stale import, a deleted
export) will look like the new fixture rejecting it.

**4. Write tests have a funded account.** This applies only if a spec signs or
submits a transaction:

```bash
grep -rEn "SubmitAndWatch|signAndSubmit|\.submit\(" e2e/
```

No hits means skip this precondition. Otherwise: TrUAPI derives a product
account from (session root, product id), and it cannot be assigned. A suite
that submits transactions signs with the DERIVED address, so that address needs
funding. Work it out with `productAccountAddress` (section 3) and fund it; do
not assume a funder seed signs. Unfunded, the first submit fails with
`Invalid.Payment` before it is broadcast.

**5. No spec depends on a refused capability.** Grep first:

```bash
grep -rEn "getPaymentLog|clearPaymentLog|setPaymentBalance|setPaymentTopUpBehavior|simulatePaymentStatus|injectChatAction" e2e/
```

Any hit is a spec that needs rewriting or skipping, not porting — see section 4.
From 0.30 (#1956) `setLoginBehavior("success" | "reject")` is served: it answers a product's
login request while the host is signed out, so a suite booting with
`loginBehavior: "manual"` can drive the cold login path. `"success"` signs in
the first account, or the one named as a second argument. The old custom
function form has no equivalent, because the answer is given in the page.
The default differs: the old host approved login requests unless told
otherwise, and this one answers `Rejected`, as a real signing host does, until
a suite calls `setLoginBehavior("success")`. A suite that relied on the old
default adds that call.

The statement-store controls are served, so they are not in this grep.
`injectChatAction` is not: the core serves chat only to a product running as a
worker, and this host always embeds the product as an iframe, so every call is
refused with `operation denied for this execution`. Treat specs that rely on it
as blocked. `injectStatement` takes `{topics, data}` the way the old package
spells it, or the SCALE wire bytes, and answers the `StatementEntry` it
retained; `getSubmittedStatements` and `getStatements` answer entries too, so a
suite reading `0x` hex off them reads `entry.data` instead.

## 1. Swap the import

```diff
-import { createTestHostFixture, type TestHost } from "@parity/host-api-test-sdk/playwright";
+import { createTestHostFixture, type TestHost } from "@parity/truapi-host/testing/playwright";
```

Network constants, the dev accounts and the log-entry types come from the same
subpath, so a file importing `PASEO_ASSET_HUB`, `DEV_ACCOUNTS`,
`DEV_ACCOUNT_NAMES`, `SigningLogEntry`, `PermissionLogEntry`,
`ChatMessageLogEntry`, `LoginBehavior`, `DevAccountName` or `fromNetworks` needs
no second import path. `createTestHostServer` is the exception: it comes from
`@parity/truapi-host/testing/server`. `LoginBehavior` here is the
`"auto" | "manual"` boot option. The type `setLoginBehavior` takes, which the
old package called `LoginBehavior`, is `LoginRequestBehavior` (0.30 and later).

`e2e/helpers.ts` usually has one `import type { TestHost }` to change too.
`HexString` is on that subpath as well, for a file annotating a `genesisHash`.

Swap the dependency in `devDependencies`, at the version chosen in
precondition 1:

```diff
-"@parity/host-api-test-sdk": "0.17.0",
+"@parity/truapi-host": "0.29.0",
```

In a pnpm repo with a catalog, use `catalog:` and set the version in the
catalog. npm does not support `catalog:`.

The package also needs `esbuild` 0.20 or later. It is an optional peer that the
server loads when it starts, so a suite without it imports fine and then fails
its first test with an error asking for esbuild as a dev dependency. Check with
`npm ls esbuild` first. Most repos already resolve one through Vite or tsx, and
pinning an older range than the one present downgrades the tree and rewrites
the lockfile. Add it only if nothing resolves.

## 2. Delete the server plumbing

The fixture starts and shares its own host server when none is given, so
`productUrl` is the only required option. That holds for one fixture call. A
suite that calls `createTestHostFixture` once per signer should keep one
module-level `createTestHostServer()` and pass its `hostUrl` to every call, or
each call starts and bundles its own server.

Only the type enforces `productUrl`: left out at runtime, the host loads
`/undefined` instead of failing.

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

Both chain spellings are accepted -- `networks: [X]` and the older `chain: X`,
which is exactly `networks: [X]` -- and either expands into the chain proxy, the
reported chain set and the runtime genesis, which have to agree. Passing both is
refused rather than merged.

The chain's role comes from the end of its `id`. An `id` ending in `-people` is
the People chain, which carries usernames, identity and the statement RPC, so
declare it whenever the product reads usernames or sends statements, not only
for `allowances: "chain"`. `NetworkConfig` has no `chain` field: the old
`chain: "People"` is a type error in an object literal and is ignored
otherwise. So an id that only got its role from that field needs the suffix,
keeping its network name, as browse's `paseo-next-v2-people` does. Take genesis
hashes from the product's descriptor bundle rather than the package constants.

Declare the chains the product reads. The old package fell back to Paseo Asset
Hub (`DEFAULT_CHAIN`) when a suite named none, so a suite that declared nothing
was on that chain. This host does not fall back: with no `networks` or `chain`,
nothing is proxied and no statement store is served, and chain reads and
statement submits fail after about 30 seconds with `HostFailure … no frame for
30s while a request waited`, unless the product's own timeout fires first. To
keep such a suite on the same network, pass `networks: [PASEO_ASSET_HUB]`.

Leave `networks` out only for a suite that reads no chain. Any flow that does
read one then waits for that failure instead of answering, so expect it to run
slower and keep assertion timeouts above it.

Two account options change shape:

- `productAccounts` has to go. It still typechecks, but the fixture throws when
  the test file loads (see section 3).
- `accounts` entries are a lower-case dev account name (`DEV_ACCOUNT_NAMES`:
  `alice`, `bob`, `charlie`, `dave`), or `{ name, entropy }` with 32 bytes of
  entropy. `uri` typechecks but throws when the fixture is built, and `username`
  is a type error. `{ name: "Alice", uri: "//Alice", username: "alice" }`
  becomes `"alice"`. An unknown name such as `"Alice"` fails the host's boot.
- The `name` is the username the host reports. To keep an old username, pair it
  with the dev account's entropy: `{ name: "alice.01", entropy: DEV_ACCOUNTS.alice }`.

What a suite that starts the server itself needs depends on the release.

- **0.30 and later (#1431):** `createTestHostServer` also takes `networks` and
  `chain`, alongside `productUrl` and `accounts`, and returns a URL ready to
  open. Chains need that `productUrl`: a server started
  without one returns a bare base the fixture configures per test, so it
  refuses `networks` and `chain` rather than dropping them, and they go to the
  fixture instead. A URL built with a `productUrl` is complete: open it
  directly, not as the fixture's `hostUrl`, whose own options replace what it
  carries.
- **0.29 and older:** the server takes host page options (`productUrl`,
  `accounts`, `mock`, `runtimeConfig`) but not `networks` or `chain`, and
  without `productUrl` it drops every option silently. Expand the networks
  yourself with `fromNetworks` and pass the `mock` and `runtimeConfig` it
  returns. Call it as `fromNetworks(networks, "default")`: its second argument,
  the loopback statement store, defaults to `false`, unlike the fixture, so a
  server built without it has no in-page statement store.

Set `productId` to the dotNS identifier the product signs with. A product that
derives its own identifier from `window.location.host` wants that value here --
under Playwright that is the dev-server origin, e.g. `"localhost:5199"`. A
`127.0.0.1` origin is not a valid product id: `productAccountAddress` says so,
but the product's own calls just hang. Serve the product on `localhost`.

Releases up to 0.28 take `productId` as a plain string. From 0.29 (#1721) it is
the branded `ProductId` type, so wrap the string with `asProductId`, which
older releases do not export:

```ts
import { asProductId } from "@parity/truapi";

createTestHostFixture({ productUrl: PRODUCT_URL, productId: asProductId("localhost:5199") });
```

The examples below use `asProductId`. Drop it on 0.28 and older. Under pnpm the
product may not depend on `@parity/truapi` directly. Add it as a dev dependency
at the same version, or skip the import and cast through the fixture's own type,
which works on every release:

```ts
import type { TestHostFixtureOptions } from "@parity/truapi-host/testing/playwright";

type ProductId = NonNullable<TestHostFixtureOptions["productId"]>;
const productId = "localhost:5199" as ProductId;
```

Without `productId` the product id is `mock.dot`, and a `productAccountAddress` call in
global setup has to use the same id the fixture runs under.

Get it wrong and every signing call fails with `PermissionDenied`, which reads
like a permission-policy problem and is not one: the core scopes a product
account to its identifier and refuses a request scoped to a different one. If
signing fails but reads pass, set this before looking anywhere else.

### Allowances are granted without being performed, by default

`allowances` defaults to `"granted"`: every allocation is answered as allocated
without being carried out, and, when the suite declares a chain that can carry
it (see below), the statement store is served in-page. A suite
that allocates allowances, signs with the key it gets back and submits
statements therefore passes with no on-chain identity.

The store is served on the People chain when the suite declares one, and on the
single declared chain when it does not -- that proxy takes every request, so it
carries the statement traffic too. Declaring several chains with no People among
them serves no store: there is no request the store could attach to, and
answering statement reads on a hub would answer what a hub refuses. Asking for
the store explicitly with `loopbackStatements: true` in that case is refused.
With no chain declared at all it is refused from 0.30 (#1431). On 0.29 and older
the suite builds and serves no store, so statement flows time out instead.

What that buys is narrower than it looks. Nothing is registered anywhere, so a
green run says the product handles a grant, not that a host would have given
one. A real statement store refuses what this one accepts.

`loopbackStatements: false` sends statement RPC to the chain proxy (the People
chain's, or the single declared chain's) instead of the in-page store. Against the real People chain under `"granted"`
the allowance key is unregistered, so the real store refuses the statements.
Point it at a store of your own, as a suite with several signers has to (see
section 4).

`"granted"` also claims no PGAS, so fees fall back to the chain's native token.
Fund product accounts with that token (PAS on Paseo), not PGAS.

Set `allowances: "chain"` for the real path -- ring membership, slot selection,
ring-VRF proof, extrinsic -- which is what a host actually does and what fails
where a host would:

```ts
createTestHostFixture({ productUrl: PRODUCT_URL, allowances: "chain" });
```

That path needs an account enrolled in a personhood ring on the target People
chain, and a People chain in `networks`. `SmartContractAllowance` also needs
the Asset Hub declared. A dev account is derived from fixed entropy and is
enrolled in none, so every ring-backed allocation (`StatementStoreAllowance`,
`BulletinAllowance`, `SmartContractAllowance`) comes back `NotAvailable`.
`AutoSigning` needs no ring and is still allocated. With `logLevel: "debug"`,
the core logs:

```
[truapi] WARN truapi::runtime::signing_host: direct resource allocation item failed
  {product_id=mock.dot, reason=signing account is not a personhood ring member;
   cannot grant statement-store allowance}
```

Pass an enrolled identity as explicit entropy to get past that. Entropy must be
exactly 32 bytes, which is a 24-word phrase. A 12-word phrase gives 16 bytes
and is rejected.

```ts
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";

accounts: [{ name: "enrolled", entropy: mnemonicToEntropy(PHRASE_24_WORDS, wordlist) }]
```

The entropy travels in the host page URL as hex, so it shows up in Playwright
traces and reports. Use an identity that holds nothing worth keeping.

`truapi-host alloc-check --mnemonic "<phrase>"` reports whether an identity is a
ring member, read-only unless `--submit` is passed. It is the Rust `truapi-host`
CLI, not part of any npm package. Install it with
`scripts/truapi-host-installer.sh` from trinity-user-agents. Run it before a browser
suite: it answers in one line what the suite takes twenty minutes to say less
clearly. Its `--network` defaults to `paseo-next-v2`, and it accepts a 12-word
phrase the fixture cannot take, so check the same phrase and network the suite
uses.

### A suite that allocates allowances on-chain must serve the People chain

Only with `allowances: "chain"`. Allowance registration reads personhood
collections from the People chain, not the hub. Serving only the hub fails every
ring-backed allocation with an outcome the product renders as "resource is
unavailable", and the core's log names the real cause:

```
[truapi] WARN truapi::runtime::statement_allowance: could not resolve this collection
  {collection=People, err=Members.Collections[People] missing}
```

Pass both, using an `id` ending in `-people` to select the role:

```ts
networks: [PASEO_ASSET_HUB, { id: "paseo-next-people", name: "Paseo People",
                    genesisHash: PEOPLE_GENESIS, rpcUrl: PEOPLE_WS }],
```

## 3. Assertions and smaller changes

**Internal storage keys.** A spec reading the old host's private key layout
(`localStorage.getItem("test-host:demo:mykey")`) is coupled to one host's
implementation. Read through the control surface instead:

```diff
-const raw = await page.evaluate(() => localStorage.getItem("test-host:demo:mykey"));
+const raw = await testHost.getProductStorageValue("mykey");
```

Both readers take the product's own key and find it inside the namespaced one,
so the caller does not need to know the namespacing and a prefixed store stays
distinct from an unprefixed one. `getProductStorageValue` returns the value
decoded as UTF-8, which is what a spec that read a `localStorage` string wants.
`findProductStorage` returns the raw `Uint8Array | undefined`, so string
assertions against it fail.

Product storage lives in the host page, so a `page.reload()` or a new page
starts it empty, where the old host kept it in the page's `localStorage`. State
a product reads once at startup has to be there before it starts, so pass it as
a boot option, by the product's own keys (0.30 and later, #1431):

```ts
createTestHostFixture({
  productUrl: PRODUCT_URL,
  productStorage: { "app:state": JSON.stringify(value) },
});
```

It travels in the host page URL, so a reload starts from it again, and
`testHost.reset()` writes it back after clearing the store.
`testHost.seedProductStorage(key, value)` (also 0.30 and later) writes an entry while the product
runs. It reaches a product that subscribes to the key or reads it later, not a
read already made at startup. Either way the host builds the key the core
reads, so a spec never spells out the namespace.

The product's own iframe storage does not carry over either: the frame is
`credentialless`, so its `localStorage` and IndexedDB start empty with each
host page and are not shared with a top-level page on the product's origin.
Seed those from inside the frame with `context.addInitScript`.

**A pinned product account.** `productAccounts: { "demo.dot/0": "bob" }`
typechecks but throws when the fixture is built, which is when the test file
loads. It cannot be supported: TrUAPI *derives* a product
account from (session root, product id), so it is not assignable. The old
package could pin one only because no core stood behind it.

Rewrite the assertion to read the address back rather than assert a known one:

With `accounts: ["bob", "charlie"]`, so the session starts as bob:

```diff
-expect(await accountAddress()).toBe(BOB_ADDRESS);
+const derived = await testHost.getProductAccountAddress();
+expect(await accountAddress()).toBe(derived);
+// and, if the test needs stability across a switch:
+await testHost.switchAccount("charlie");
+expect(await testHost.getProductAccountAddress()).not.toBe(derived);
+await testHost.switchAccount("bob");
+expect(await testHost.getProductAccountAddress()).toBe(derived);  // derivation is deterministic
```

Switching the host account **changes** the product account. Any spec asserting
it stays fixed is asserting the old package's behaviour, not the protocol's.
`switchAccount` takes only a name on the current roster (the fixture's
`accounts`, or the last `setAccounts`) and rejects with `no account "<name>" on
this host; have …` for any other. It does not reload the product, so a product that caches its
account needs the spec to reload or re-read it.

The fixture address is SS58 prefix 42 (`5…`). A product rendering another
prefix shows a different string for the same account, so compare decoded
public keys, not strings, when the two have to agree.

Addresses also differ from polkadot-js `//Alice` and friends: a session
activates from 32 bytes of entropy, not a derivation path. Never hard-code a
well-known address; read it back.

Usernames change too. The host reports the account `name` (`alice`), where the
old package reported a numbered one (`alice.01`) or the `username` a suite set.
Either update specs that assert the old form, or keep it by naming the account
after it with the dev entropy: `{ name: "alice.01", entropy: DEV_ACCOUNTS.alice }`.
`TestHost` has no username getter.

People-chain identity and usernames bind to the session root, not the product
account, and nothing in the package returns the root's public key. A suite that
registers identities on a fork has to derive it from the account's entropy
itself.

**A permission revoked mid-run.** The core persists a decided authorization per
(product, permission) and answers from its own storage, so a product asking
twice is answered from that record and the host is not called again.
`grantPermission` and `revokePermission` are the suite talking to the host:
they retract that record, so the next request is put to the host afresh.
`resetPermission` does the same but exists only on the page's
`window.__TRUAPI_TEST_HOST__`, not on the fixture. `setPermissionBehavior` and `setEnforcePermissions` do not
retract it, so a decision already stored still wins after them.

In a spec, with the request driven through the product's UI:

```ts
await requestCamera(page);                   // asks the host
await requestCamera(page);                   // answered from the record

await testHost.revokePermission("Camera");   // returns a Promise: await it
await requestCamera(page);                   // asks again, denied
```

`setPermissionBehavior` takes `"allow-all"`, `"deny-all"` or `"reject-all"`,
which is another name for deny. The old package's `"approve-all"` and
`"approve-once"` are type errors, and at runtime the returned promise rejects
with `does not know the policy "approve-all"`.

A spec that revokes and then waits for a host-side entry works. One that expects
a *product* asking twice to re-prompt does not, and is asserting the old
package's behaviour.

### Smaller changes

Each of these is a one-line edit, but each fails quietly or confusingly if
missed:

- **Report unserved chains as a real host does.** By default the mock reports
  every chain as supported and leaves an unserved one silent, so a product's
  "network unavailable" path hangs. New suites should pass
  `mock: { onlyServedChains: true }` (0.30 and later: #1432, which reached main
  through #1431), which answers
  from the chains the suite declares. It becomes the default in a later
  breaking release.
- **Role suffixes.** Besides `-people`, an id ending in `-asset-hub` or
  `-bulletin` selects that role. Anything else is a relay chain, so ids such as
  `asset-hub` or `people` with no network prefix are all read as relay chains.
- **`PASEO_ASSET_HUB` carries a new genesis.** It is re-pinned to the hash the
  chain reports since its reset, so a product or suite still holding an older
  copy of the hash disagrees with it, and a suite that spreads it and overrides
  `rpcUrl` declares a genesis that endpoint may not serve.
- **Removed exports.** `PREVIEWNET`, `PREVIEWNET_ASSET_HUB`,
  `SUPPORTED_CHAINS`, `ChainConfig`, `Account` and `DevAccountInfo` are not
  exported. Declare the chain inline, type it as `NetworkConfig`, and use
  `DevAccount`.
- **Permission tags are the core's,** so `"TransactionSubmit"` is
  `"ChainSubmit"`, and `grantPermission` with the old tag records a decision no
  request ever matches.
- **Themes are capitalised:** `setTheme("Dark")`, not `"dark"`.
- **Declining a signature in one test.** From 0.30 (#1926), call `testHost.setConfirmUserActions(false)`
  before the action and `true` after, rather than booting a second fixture
  with `mock: { confirmUserActions: false }`. A permission the core has
  already cached keeps its answer.
- **Signed out is a boot option.** An empty `accounts` fails the page at boot,
  which a suite sees as the fixture timing out. Pass `loginBehavior: "manual"`.
  `simulateDisconnect` drops the chain connections and refuses new ones until
  `simulateReconnect`. Neither touches the session, so a spec that used them to
  sign out needs `loginBehavior: "manual"` instead.
- **After `switchAccount`,** wait on the product's own UI for the new account
  rather than on `waitForConnection`, which has already resolved.
- **Seed preimages whenever.** `seedPreimage` reaches a lookup the product
  already has open (0.30 and later, #1431), so a spec need not seed before the
  product asks. It returns the key as bytes, not hex.
- **Log shapes.** `getNavigationLog()` answers URLs, not `{url, timestamp}`.
  `getPreimages()` answers the seeded values only, as `Uint8Array[]`, with no
  `key`, `fromProduct` or `timestamp`. Statement entries carry `topics` and
  `data` directly, not under `.statement`. From 0.30 they also carry
  `channel`, `expiry` (a bigint) and `decryptionKey`, which `injectStatement`
  also takes (#1935). A `Remote` permission log entry's `value` is the tagged
  permission `{ tag: "Remote", value: { domains } }`, so read
  `entry.value.value.domains`.
- **Removed members.** Some old `TestHostAPI` members have no equivalent, and a
  spec using one fails with `TS2339`: `clearChat`, `clearOperationLog`,
  `clearProductStorage`, `clearResourceAllocationLog`,
  `clearUserConfirmationLog`, `getDevicePermissionStatuses`,
  `getFeatureSupport`, `getLocale`, `getOpenOperations`, `getOperationLog`,
  `getProductStorageEntries`, `getResourceAllocationLog`, `getSupportedChains`,
  `getUserConfirmationLog`, `seedChatBot`, `seedChatRoom`,
  `setDevicePermissionStatus`, `setFeatureSupport`, `setLocale`,
  `setNavigationBehavior`, `setNotificationBehavior`,
  `setResourceAllocationBehavior`, `setSupportedChains` and
  `setUserConfirmationBehavior`. `seedProductStorage` returns in 0.30. Skip or
  rewrite those specs. `window.__TEST_HOST__` still exists, but it now names the
  same object as `window.__TRUAPI_TEST_HOST__`, with these names and the shapes
  above, and the old `getSentRpc` is `sentRpc` there.
- **Deep links.** Up to 0.29 the host page's own path, query and hash configure
  the host and do not reach the product, so a spec opening the product at a
  route passes that route in `productUrl`. From 0.30 (#1932) the host page
  passes its path, its hash and every query parameter it does not read itself
  on to the product, as a real host does. With the fixture, call
  `testHost.openDeepLink("/notes/1?view=raw#top")`, which loads the host page
  again at that link. With the server alone, open `deepLinkUrl(server.url, link)`
  from `@parity/truapi-host/testing/server`.
- **Full-page navigation.** A product that navigates its whole document
  (`location.href = ...`) reconnects only when both `@parity/truapi-host` and
  the product's own `@parity/truapi` are 0.30 or later (#1895), since part of
  the fix ships in the product's SDK. Before that it loses the host after the
  first navigation.

### A suite that submits extrinsics must fund the product account first

A product account is derived, so it starts with no balance and nothing has ever
funded it. A suite that submits a real extrinsic fails within seconds with
`Invalid.Payment` -- before it is broadcast, with the rest of the protocol path
working, which reads like a host bug and is not one. The old package sidestepped
this by pinning the product to an already-funded dev key.

Work the address out before the run, not from the failures. This needs no host
and no browser:

```ts
import { asProductId } from "@parity/truapi";
import { productAccountAddress } from "@parity/truapi-host/testing/playwright";

const address = await productAccountAddress({
  account: "bob",                         // the dev account the session activates from
  productId: asProductId("tx-demo.dot"),  // index defaults to 0
});
```

Fund that address once, from a faucet or a transfer, with the native token
(see "Allowances" in section 2). Derivation is deterministic, so it holds for
every later run. Re-fund only when the suite changes its `accounts` or its
product id: both feed the derivation, so either change is a different account.
A product that signs from several indices needs each `index` funded. A suite
on a fork can instead top up whatever signs as it submits.

Because the address is the same on every run, two CI runs at once share its
balance and nonces. Put them in one concurrency group, or give each run its own
entropy.

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

Tags are not validated, so a misspelt one withholds nothing. The valid tags are
`StatementStoreAllowance`, `BulletinAllowance`, `SmartContractAllowance`, which
covers every index, and `AutoSigning`.

## 4. What refuses, and why

Five of `TestHostAPI`'s members, the payment ones, throw with an explanation
rather than returning something plausible. The rest, including the
statement-store controls, are served. `injectChatAction` is the exception noted
in precondition 5.

- payments (`getPaymentLog`, `setPaymentBalance`, `simulatePaymentStatus`,
  `setPaymentTopUpBehavior`, `clearPaymentLog`) -- only native and Rust hosts
  serve payments, because the browser host-callback bridge cannot carry the
  payment platforms yet. This host answers every payment call `Unsupported`,
  so there is no payment engine to drive. A suite with payment specs cannot
  migrate them; it can only drop or rewrite them.

A test reaching one of these gets a reason, not `is not a function`. They throw
synchronously rather than returning a rejected promise, so assert with
`expect(() => testHost.getPaymentLog()).toThrow()`, not `.rejects`.

### Several signers in one test

Each fixture call drives one page, and each page keeps its own in-page
statement store, so two signers never see each other's statements. A suite with
several signers:

- uses one browser context and one fixture call per signer, sharing one
  `hostUrl` (section 2);
- sets `loopbackStatements: false` and serves one shared statement store behind
  the People RPC every host declares;
- includes the retry in what it creates, because that shared store keeps what a
  failed attempt submitted.

## 5. Verify

Run the suite and compare against its pre-migration result, not against green,
with the same command as precondition 3. Run the whole suite, not single specs,
since some specs rely on state an earlier one created.

Expect the same passes, minus anything in section 4. If the product shows
connected but no call is ever answered, and the host logs `invalid frame: frame
is missing the message-type byte`, re-check step 0 -- that is the codec
symptom, not a fixture problem.

If the host fails to boot, for example on an unknown account name, the fixture
does not fail fast. It waits `readyTimeoutMs` (30 s by default, the same as
Playwright's default test timeout), so the test usually times out first. The
reason is in the host page's `#test-host-error` element.

### When a call fails and the product only says "error"

The core logs why a call failed before mapping it to a protocol answer. In
Chromium, core log lines reach `page.on("console")` in the default worker
topology too (the `logLevel` option's JSDoc says otherwise and is out of date),
but running on the main thread keeps them in order with the page's own output.
Raise the level:

```ts
createTestHostFixture({
  productUrl: PRODUCT_URL,
  topology: "main-thread",
  logLevel: "debug",
});
```

The reason then arrives on the page console, for example:

```
[truapi] WARN truapi::runtime::signing_host: direct resource allocation item failed
  {product_id=mock.dot, reason=signing account is not a personhood ring member;
   cannot grant statement-store allowance}
```

Both are debugging aids. `"worker"` is the production topology, so take them
back out once the failure is understood.
