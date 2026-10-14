# Migrating product-sdk's e2e suites onto the TrUAPI test host

`product-sdk-truapi-migration.patch` moves product-sdk's nine example suites off
`@parity/host-api-test-sdk` and onto `@parity/truapi-host/testing`. It applies
cleanly to product-sdk `origin/main` (verified with `git apply --check`), and is
a starting point for review rather than a finished PR — see the open items at
the end.

Apply from the product-sdk repo root:

```bash
git apply docs/migration/product-sdk-truapi-migration.patch
```

## What is in it

**Every suite: the fixture, and only the fixture.** `e2e/fixtures.ts` builds its
`testHost` from `@parity/truapi-host/testing/playwright` instead, and
`e2e/helpers.ts` changes one type import. The specs are otherwise untouched —
the method names on the new fixture deliberately match `TestHostAPI`'s so a
migrating suite changes its import, not its assertions.

No server plumbing: `productUrl` is the only required option, and the fixture
starts and shares its own host server when none is given. So a suite's whole
diff is typically the import line:

```diff
-import { createTestHostFixture, type TestHost } from "@parity/host-api-test-sdk/playwright";
+import { createTestHostFixture, type TestHost } from "@parity/truapi-host/testing/playwright";
```

`networks` is accepted in the old `NetworkConfig` shape and expanded into the
proxy, chain set and runtime genesis that have to agree, so a suite passing
`PASEO_ASSET_HUB` keeps working. `productAccounts` and an account given by a
derivation `uri` are rejected at construction, each naming what to do instead —
see the two rewrites below for why neither can be served.

**Five assertion rewrites, all one cause.** Four suites read
`@parity/host-api-test-sdk`'s *internal* storage keys out of the host page —
`localStorage.getItem("test-host:demo:mykey")` and friends. That is a test
coupled to one host's implementation rather than to product behaviour, and no
amount of API compatibility ports it. They now read through the control surface
(`findProductStorage`), which is the host's public contract:

- `storage-demo`: `kv-ops.spec.ts`, `prefix.spec.ts`
- `keys-demo`: `session.spec.ts`
- `host-demo`: `storage-ops.spec.ts`
- `signer-demo`: `persistence.spec.ts` (its `STORAGE_KEY` constant also carried
  the `test-host:` prefix, and it used that storage as a flush barrier)

**Account switching needs no rewrite.** `signer-demo/switch-account.spec.ts`
asserts that switching the host account leaves the dapp-scoped product account
stable, and it passes here unchanged. Measured rather than assumed: switching
moves the host's active account (`getActiveAccount` goes `alice` -> `charlie`)
while the address the signer surfaces does not move. The product reads its
product account once and holds it, so the observable behaviour matches.

**One skip, proposed rather than decided.** `signer-demo/permission.spec.ts`
asserts that revoking a permission mid-run and reconnecting re-prompts the host.
A real TrUAPI core persists a decided authorization per (product, permission)
and answers from its own storage, so the second request never reaches the host.
It passes today only because `@parity/host-api-test-sdk` is a TypeScript
reimplementation of the protocol with no core behind it. No test host can make
it pass; the comment says so and the call is product-sdk's.

## Results

Against the TrUAPI test host, on the production Web Worker topology:

| Suite | Result |
| --- | --- |
| storage-demo | 8/8 |
| keys-demo | 8/8 |
| host-demo | 6/6 |
| signer-demo | 8/9, 1 skipped as above |
| chain-client-demo | 2/2, proxying the real Asset Hub |
| tx-demo | boots; writes blocked on unfunded accounts |
| contracts-demo | boots to `pallet-revive` account mapping, a write |
| statement-store-demo | connects over the real people chain; 3 control methods absent |
| cloud-storage-demo | already skipped upstream |

## Open items, which is why this is not a finished PR

- **Apply this only after the codec bump lands — it is a hard ordering
  dependency, not a preference.** The catalog keeps `"@parity/truapi": ^0.13.1`,
  which is wire codec 1, while the test host this patch switches to is codec 2.
  Codec 1 and 2 cannot negotiate: the handshake is refused with
  `UnsupportedProtocolVersion`. Applied on its own, this patch therefore produces
  a handshake failure that reads like a broken test host. `@parity/truapi` must
  reach 0.16.0 first (product-sdk #376).
- **Catalog pin.** `pnpm-workspace.yaml` gains `"@parity/truapi-host": ^0.18.0`.
  0.18.0 is the release that carries the `./testing` subpath; no published
  version before it has one. `@parity/truapi-host` also published 0.10.1 and
  then 0.16.0 with nothing in between, because 0.11 through 0.15 were cut
  in-repo and never reached npm, so a pin anywhere in that range fails to
  install rather than resolving to something older.
- **product-sdk does not compile against `@parity/truapi` 0.15+ as-is.**
  `packages/host/src/testing.ts` (`createFakeHost`) is missing
  `signRawUnwatermarkedDeprecated` and its `LegacyAccount` twin, and
  `PublicTruApiClient` is missing the `renderer` domain. Both are small, both
  are prerequisites, and neither is in this patch because they are a separate
  change: the codec-1 → codec-2 bump.
- **Write-dependent suites need funded accounts.** `tx-demo` and
  `contracts-demo` reach the chain and fail on fees. The addresses are
  deterministic per (dev account, product id) and funding them is an
  operational decision, not a code one.
