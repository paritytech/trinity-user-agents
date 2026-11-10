# Moving your e2e suite to the TrUAPI test host

`@parity/host-api-test-sdk` is being retired in favour of `@parity/truapi-host/testing`
([#1349](https://github.com/paritytech/trinity-user-agents/issues/1349)). The new test host runs the real TrUAPI core
and mocks only the platform. It ships with every core release, so your suite tests what your users run.

Most specs stay as they are. The fixture keeps most of the old method names, and step 5 lists the ones that changed or
went away.

## 1. Check you can move

Your product must use `@parity/truapi` 0.24 or later:

```bash
pnpm why @parity/truapi   # or: npm ls @parity/truapi
```

Several copies can show up, for example under the old test SDK or under a tool that brings its own product SDK. Read the
copy resolved by the product SDK your `package.json` lists directly (`@parity/product-sdk-host` or
`@parity/product-sdk`).

| Your stack                                                                                                | What to do                                                            |
| --------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| product-sdk-host 0.25 or later, or product-sdk 0.34 or later (truapi 0.24 or later)                       | Move now                                                              |
| product-sdk-host 0.24 or older, product-sdk 0.33 or older, or `@novasamatech/host-api` in your own source | Upgrade the product SDK first, and stay on the old package until then |

Older products either speak an older wire codec or have no `truapi-host` release with the test host features this guide
uses, which first appear in 0.24. A product on truapi 0.13 or older reports itself connected, but no call is ever
answered, and the host logs `invalid frame: frame is missing the message-type byte`.

Also run your suite today with its usual command, for example `npm run test:e2e`, and note the result. You will compare
against it.

## 2. Swap the dependency

```diff
-"@parity/host-api-test-sdk": "...",
+"@parity/truapi-host": "<the same minor as your @parity/truapi>",
```

Use the `truapi-host` release with the same minor as the `@parity/truapi` your product resolves. The two are released
together, and each release is tested as that pair. An exact pin is fine. If your repo pins `@parity/truapi` with an
`overrides` entry, move it together with `truapi-host`.

Keep it in `devDependencies`, where the old package was. Use `catalog:` only if your repo uses pnpm catalogs.

The test host also needs `esbuild` 0.20 or later, which it loads when the server starts. Check with `npm ls esbuild`.
Add it as a dev dependency only if nothing resolves, and don't pin a range older than the one you already have. Without
it, the first test fails with `@parity/truapi-host/testing/server needs esbuild`.

```diff
-import { createTestHostFixture, type TestHost } from "@parity/host-api-test-sdk/playwright";
+import { createTestHostFixture, type TestHost } from "@parity/truapi-host/testing/playwright";
```

`PASEO_ASSET_HUB`, `DEV_ACCOUNTS`, `fromNetworks` and the log types come from the same path. `DEFAULT_CHAIN` is the same
chain, and `LIVE_CHAINS.paseoAssetHub` holds its RPC URL and genesis for `liveChain(...)`, not a `NetworkConfig`. No
other chain is exported, so declare other chains yourself as `NetworkConfig`. `createTestHostServer` comes from
`@parity/truapi-host/testing/server`.

## 3. Configure the fixture

```ts
import { test as base } from "@playwright/test";
import { asProductId } from "@parity/truapi";
import { createTestHostFixture, PASEO_ASSET_HUB, type TestHost } from "@parity/truapi-host/testing/playwright";

const PRODUCT_URL = "http://localhost:3000";

export const test = base.extend<{ testHost: TestHost }>(
  createTestHostFixture({
    productUrl: PRODUCT_URL,
    productId: asProductId("myapp.dot"),
    networks: [PASEO_ASSET_HUB],
    accounts: ["alice"],
  }),
);
```

- **`productUrl`** is required. Only the type checks it, so a missing value loads `/undefined` rather than failing.
- **`productId`** must be the identifier your product signs with, as a dotNS name or `localhost:<port>`. It defaults to
  `mock.dot`. A wrong value fails every signing call with `PermissionDenied` while reads still work. `127.0.0.1:<port>`
  is not a valid id, so use `localhost:<port>`, and if your product derives its id from its own origin, serve it on
  `localhost` too. From 0.29 the id is typed as `ProductId`, so wrap the string with `asProductId` as above. On 0.28 and
  older, pass the plain string. If your repo doesn't depend on `@parity/truapi` directly, add it as a dev dependency at
  the same version.
- **`networks`** lists the chains your product reads. The old package fell back to Paseo Asset Hub when a suite named
  none. This one doesn't, so a suite that relied on that default needs `networks: [PASEO_ASSET_HUB]` to stay on the same
  network. With no `networks`, chain reads and statement submits fail after about 30 seconds with
  `HostFailure … no frame for 30s while a request waited`, unless your product times out sooner. Leave it out only if
  your suite reads no chain.
- **Chain roles** come from the end of each `id`. An id ending in `-people` is the People chain, which carries usernames
  and statements, so declare it if your product reads either. Likewise `-asset-hub` and `-bulletin` select those roles,
  and any other id is read as a relay chain, so a bare `people` or `asset-hub` id needs a network prefix such as
  `paseo-next-people`. Remove any old `chain: "People"` field, which is now a type error. Take genesis hashes from your
  product's own descriptors.
- **`productAccounts`**: delete it. It still typechecks with string values, but the fixture throws when the test file
  loads, because a product account can't be pinned (step 4).
- **`accounts`** takes the lower-case dev account names `"alice"`, `"bob"`, `"charlie"` and `"dave"`
  (`DEV_ACCOUNT_NAMES`), or `{ name, entropy }` with 32 bytes of entropy. `eve` and `ferdie` are gone and fail at boot,
  so give them entropy of your own. The `name` is also the username the host reports. `uri` throws when the fixture is
  built, and `username` is a type error. So `{ name: "Alice", uri: "//Alice", username: "alice" }` becomes `"alice"`. To
  keep an old username such as `alice.01`, use `{ name: "alice.01", entropy: DEV_ACCOUNTS.alice }`. Each `DEV_ACCOUNTS`
  entry is that entropy as a `Uint8Array`, not the old `{ name, uri }`.
- **Old boot options.** `executionKind`, `initialState` and most of `behaviors` are gone and are type errors:
  - `initialState.productStorage` is the `productStorage` option (0.30 and later), keyed by the product's own keys.
  - `initialState.theme` and `initialState.locale` are `mock: { theme, languageTag }`, and `initialState.features` is
    `mock: { featureSupported }`, one answer for every feature. `initialState.supportedChains` is
    `mock: { supportedChains }`.
  - `initialState.grantedPermissions` is a `testHost.grantPermission(...)` call per permission.
  - `behaviors.permission` is `testHost.setPermissionBehavior(...)` (see step 5), and
    `behaviors.userConfirmation: "reject-all"` is `mock: { confirmUserActions: false }`.
  - `behaviors.resourceAllocation` takes only the record form, naming the resources the host refuses.
  - `executionKind: "Worker"`, `behaviors.navigation`, `behaviors.notification` and
    `initialState.devicePermissionStatuses` have no equivalent.
- **Server.** Delete any `createTestHostServer()` and `hostUrl` plumbing if you call the fixture once, since it starts
  its own server. If you call it once per signer, start one shared server with no options and pass its `url` as each
  call's `hostUrl`, or each call starts its own. The fixture options, `networks` included, then go to each fixture call.
  Pass options to the server only when your suite opens `server.url` itself, without the fixture, and then always pass
  `productUrl` with them. From 0.30 such a server takes `networks` too
  ([#1431](https://github.com/paritytech/trinity-user-agents/pull/1431)). On 0.29 and older, expand them with
  `fromNetworks(networks, "default")` and pass the `mock` and `runtimeConfig` it returns.
- **`allowances`** defaults to `"granted"`: the host answers every allocation as allocated without performing it, and
  claims no PGAS. `"chain"` runs the real path and needs an account enrolled in a People ring. The migration skill
  (`.claude/skills/migrate-to-truapi-test-host/SKILL.md`) covers it.

## 4. Fund the product account

A suite that never signs or submits an extrinsic can skip this step.

The product account is derived from the session and the product id. It cannot be pinned to `//Alice` or any other known
key. Work out the address and fund it, or the first extrinsic fails with `Invalid.Payment` before it is broadcast:

```ts
import { asProductId } from "@parity/truapi";
import { productAccountAddress } from "@parity/truapi-host/testing/playwright";

const address = await productAccountAddress({ account: "alice", productId: asProductId("myapp.dot"), index: 0 });
```

- Fund it once with the chain's native token (PAS on Paseo), from a faucet or a transfer. Under the default
  `allowances: "granted"` the host claims no PGAS, so fees fall back to the native token. `productAccountAddress` needs
  no browser, so a script or global setup can print the address to fund.
- If your product signs from several indices, fund each one (`index` in the call above), or run against a fork that tops
  up whatever signs.
- The address is stable for the same account and product id. On a live chain, check its balance in global setup so a
  drained account fails clearly. Inside a test, `testHost.getProductAccountAddress()` returns the same address.
- Because the address is the same on every run, two CI runs at once share its balance and nonces. Run them in one
  concurrency group, or give each run its own entropy.

## 5. Update the assertions that change

- **Accounts.** Read addresses back instead of hard-coding them. Switching the host account changes the product account.
- **Usernames.** The host reports the account `name`, so `alice` where the old host said `alice.01` (from 0.14) or
  `Alice` (0.12). Update the assertion, or keep the old form as in step 3.
- **Storage.** Read storage with `testHost.getProductStorageValue("key")`, not the old host's `localStorage` keys. It
  returns a string. `findProductStorage` returns the raw bytes. Product storage starts empty on every page load. From
  0.30, seed it with the `productStorage` fixture option or `testHost.seedProductStorage(key, value)`. Both take the
  product's own key, not the namespaced key the old `getProductStorage()` reported, so an old namespaced key still
  typechecks and writes where the product never reads.
- **Connection status.** `getConnectionStatus()` and `getChainStatus()` report the chain, as `"Connected"`,
  `"Disconnected"` or `"Idle"`, where the old `getConnectionStatus()` reported the product as `"connected"`. An
  assertion on `"connected"` still typechecks and then fails. Wait for the product with `testHost.waitForConnection()`.
- **Permissions.** A permission is asked once and remembered. `await testHost.revokePermission(...)` or
  `grantPermission(...)` makes the next request reach the host again. `setPermissionBehavior` doesn't. It takes
  `"allow-all"` or `"deny-all"` (`"reject-all"` is another name for deny), so the old `"approve-all"` and
  `"approve-once"` fail. `grantPermission` takes the permission name only, so drop a second argument such as
  `{ domains }`. A `Remote` grant covers every domain.
- **Signed out.** An empty `accounts` fails at boot. Use `loginBehavior: "manual"`. `simulateDisconnect` drops only the
  chain connection and doesn't sign the user out, unlike the old package up to 0.12.
- **Approving a login.** From 0.30, boot with `loginBehavior: "manual"` and call `testHost.setLoginBehavior("success")`,
  or `"reject"`, to answer the product's login request. Until a test calls it, the host answers `Rejected`. The old
  package up to 0.12 approved by default and also took a function, which has no equivalent. The old package from 0.13
  had no login control. On 0.29 and older nothing can answer a login request, so skip those specs.
- **Signing logs.** Don't assert exact signing-log counts. The new host can record more than one entry for a flow.
- **Declining one signature.** From 0.30 call `testHost.setConfirmUserActions(false)` before the action and `true`
  after, rather than booting a second fixture with `mock: { confirmUserActions: false }`.
- **Statements and preimages.** `getSubmittedStatements` returns entries with `topics` and `data` at the top level, as
  the old package does from 0.14. Up to 0.12 they sat under `.statement`. `seedPreimage` returns the key as bytes, not
  hex. `getPreimages` returns the values only, as `Uint8Array[]`, with no `key`, `fromProduct` or `timestamp`. From 0.30
  statement entries also carry `channel`, `expiry` (a bigint) and `decryptionKey`, which `injectStatement` also takes.
- **Removed methods.** These old methods have no equivalent, so skip or rewrite specs that use them: `clearChat`,
  `clearOperationLog`, `clearProductStorage`, `clearResourceAllocationLog`, `clearUserConfirmationLog`,
  `getDevicePermissionStatuses`, `getFeatureSupport`, `getLocale`, `getOpenOperations`, `getOperationLog`,
  `getProductStorageEntries`, `getResourceAllocationLog`, `getSupportedChains`, `getUserConfirmationLog`, `seedChatBot`,
  `seedChatRoom`, `setDevicePermissionStatus`, `setFeatureSupport`, `setLocale`, `setNavigationBehavior`,
  `setNotificationBehavior`, `setResourceAllocationBehavior`, `setSupportedChains` and `setUserConfirmationBehavior`.
  `seedProductStorage` is available again from 0.30.
- **Deep links.** Up to 0.29 the host page's own path, query and hash don't reach the product, so put the route in
  `productUrl`, with one fixture per link. From 0.30 call `testHost.openDeepLink("/notes/1?view=raw#top")`, or open
  `deepLinkUrl(server.url, link)` from `@parity/truapi-host/testing/server` when you start the server yourself.
- **Smaller renames.** Themes are capitalised (`setTheme("Dark")`), and `getTheme()` returns the name as a string.
  `getProductStorage()` returns `Uint8Array` values, not strings. `getNavigationLog()` returns URLs. `Account` is no
  longer exported, so use `DevAccountName | DevAccount`, and declare chains as `NetworkConfig`. `DevAccountInfo`,
  `InitialState`, `InitialBehaviors`, `ProductStorageEntry`, `ResourceAllocationLogEntry`, `UserConfirmationLogEntry`,
  `SUPPORTED_CHAINS`, the `PREVIEWNET*` constants and `TRUAPI_WIRE_SCHEMA_HASH` are gone too.

## 6. What isn't supported yet

- **Payments.** Every payment call answers `Unsupported`, and the fixture's payment methods throw straight away. Drop or
  skip those specs.
- **Chat actions.** `injectChatAction` is always refused with `operation denied for this execution`, because the test
  host runs the product in an iframe and chat is served only to workers. Skip specs that rely on it.
- **Several hosts sharing statements.** Each page keeps its own statement store, so two signers in one test don't see
  each other's statements. Send them to a shared store behind your People RPC with `loopbackStatements: false`. A real
  People chain refuses them under the default `allowances: "granted"`, so that store has to be your own.
- **Full-page navigation.** A product that sets `location.href` reconnects only when both `@parity/truapi-host` and the
  product's own `@parity/truapi` are 0.30 or later. Until your product SDK brings truapi 0.30, skip those specs.

## 7. Keep the rest the same

- Run on the same network, with the same command and worker count as before. A migration should change only the test
  host.
- If your suite creates names or on-chain state, such as usernames or registrations, include the retry in them (for
  example `${runId}-${testInfo.retry}`), so a retried test doesn't collide with what the failed attempt left behind. A
  shared statement store also keeps what the failed attempt submitted. Suites that create nothing can ignore this.
- Keep mnemonics and entropy out of the host page URL. They end up in Playwright traces.

## 8. Verify

Run the full suite with the same command you used in step 1, not a single spec, since some specs rely on state an
earlier one created. Compare it with the result you noted. `window.__TEST_HOST__` still exists in the browser, with the
method names and shapes from step 5, and the fixture's `getSentRpc` is `sentRpc` there.

If the host fails to boot, for example on an unknown account name, the fixture waits 30 seconds, which is also
Playwright's default test timeout, so the test times out without saying why. The reason is in the page's
`#test-host-error` element. Pass a shorter `readyTimeoutMs` to the fixture to fail sooner.

To see why a call failed, raise the core's log level while debugging. Running it on the page keeps its lines in order
with your product's own output. Remove both options again afterwards:

```ts
createTestHostFixture({ productUrl: PRODUCT_URL, topology: "main-thread", logLevel: "debug" });
```

## Help

- Example migrations: [paritytech/browse#141](https://github.com/paritytech/browse/pull/141),
  [paritytech/coffer#201](https://github.com/paritytech/coffer/pull/201)
- Tracking: [#1349](https://github.com/paritytech/trinity-user-agents/issues/1349), with one issue per product
- Questions: comment on your product's migration issue
