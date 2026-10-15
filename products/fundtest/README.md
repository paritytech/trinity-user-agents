# fundtest: reference funding provider for Paseo Next v2

`fundtest` is a TrUAPI product that serves the funding modality on the Paseo Next v2 testnet, so the CASH card and the
funding overlay can be run end to end on a dev device. The hosts bundle it into their testnet builds (below), so it
needs no dotNS name or deployment. It simulates only the off-chain leg (the card charge, the bank transfer, the payout).
Everything on chain is real: the money a payment would bring is PAS from the public Paseo faucet, dripped to a
per-session account on Asset Hub, getcash's pipeline swaps it to CASH and sends it to People, and `payment.topUp` claims
it into the user's balance.

The product has two executables:

- **Worker** (`worker/`): serves the `FundingProvider` trait. It answers quote asks, runs each assigned session and
  reports its progress.
- **App** (`app/`): the provider's own screens, which the worker shows with `presentFrame`: `/card` (card form and Pay),
  `/bank` (bank details and "I've sent funds") and `/payout` (where an outbound payout goes).

## What is real and what is simulated

| Step                                          | Real or simulated                                                          |
| --------------------------------------------- | -------------------------------------------------------------------------- |
| Quotes                                        | Simulated: fixed prices (below), answered from the worker                  |
| Card charge, bank transfer                    | Simulated: a screen and a wait                                             |
| The deposit reaching Asset Hub                | Real: a public faucet drip of PAS to the session account                   |
| A crypto deposit                              | Real: from the user's wallet, or a faucet drip of PAS in demo mode         |
| Conversion on Asset Hub, transfer to People   | Real: getcash's pipeline, one `PolkadotXcm.execute` by the session account |
| Crediting the user                            | Real: `payment.topUp` with the session account's key                       |
| Collecting an outbound payment                | Real: `payment.request` to the provider's reserve account                  |
| Paying the user out after an outbound session | Simulated: a wait; the CASH stays with the reserve account                 |

Prices are fixed so a run is predictable: 1 CASH costs 0.99 EUR, the provider takes 1%, and each rail adds a fixed
network fee (0.10 EUR on card and bank, a small amount of the asset on crypto). Crypto assets are priced at 1 CASH per
USDT, USDC and CASH, and 4 CASH per PAS. A quote holds for 60 seconds and gives an ETA by rail: 5 minutes for card, 2
days for bank, 10 minutes for crypto. Amounts below 10 CASH or above 5000 CASH are refused with `BelowMinimum` or
`AboveMaximum`, and card and bank are refused with `CountryUnsupported` outside the countries the manifest lists (the
United States is left out on purpose). The deposit a crypto session asks for is not the quote's figure: it is sized live
from the chain so the conversion lands the full amount.

### Picking a failure case

The cents of the amount choose what goes wrong:

| Cents | Inbound                                                         | Outbound                                                                                                    |
| ----- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `.13` | The payment is seen, then refunded: `Failed { Refunded }`       | The payout fails after the release; the reserve account refunds the user and the worker reports `Refunding` |
| `.23` | The credit is split over two top-ups, so the host shows a retry | No change                                                                                                   |
| `.33` | The payment arrives only after the quoted ETA has passed        | The payout arrives after the ETA                                                                            |
| `.43` | A card or bank payment is declined: `Failed { Declined }`       | No change                                                                                                   |
| `.63` | No change                                                       | The payout is retried twice, each reported as `PayoutRetrying`, then goes through                           |

A crypto deposit made from a wallet ignores `.13`: the refund case only runs in demo mode, before any drip. A bank
payment quotes a two-day ETA, so `.33` on bank is seen through only with a short `FUNDTEST_BANK_ETA_SECS`.

### Stopping a session as if the worker crashed

Cents `.51` to `.54` stop the session once at a kill point, just after an external action and before the step after it
is saved. The worker drops what it held for the session in memory and opens its stream again, and the host hands the
session back with what it saved, as it would after a restart. Force-quitting the app at any point exercises the same
path.

| Cents | Inbound                            | Outbound                        |
| ----- | ---------------------------------- | ------------------------------- |
| `.51` | After the faucet drip is asked for | After `payment.request`         |
| `.52` | After the first conversion tick    | After `Collecting` is reported  |
| `.53` | After `payment.topUp`              | After the payout's `Converting` |
| `.54` | After `Crediting` is reported      | No stop                         |

A session stops at its kill point only once; the mark is kept in product storage under `fundtest:killed:<intent>`.

## How each rail runs

Every session first derives its own account on Asset Hub from host entropy under this product, with the label
`onramp:eph:fundtest:<intent>`, the way getcash derives its burners. The host derives the same account for the same
user, product and label, so the account survives a restart and its secret is never stored. A second account per user,
the reserve (label `onramp:eph:fundtest:collection`), collects outbound payments and the PAS a drip leaves over.

**Card in.** `AwaitingPayment` and `Details`, then `/card`. Pay first asks the user to allow network access to the
faucet (below), then a short wait stands in for the card network and the session reports
`PaymentReceived { finalized: false }`. The worker sizes a PAS deposit for the pool route with getcash's sizing, and the
session account asks the faucet for a drip. Once the drip lands, the session account sends everything above the deposit
to the reserve, because getcash's pool tier converts the whole native balance. Once the deposit is final it reports
`PaymentReceived { finalized: true }` and `Converting`, swaps the PAS to CASH through the asset-conversion pool and
sends the CASH to People in one `PolkadotXcm.execute`, claims it with `payment.topUp`, and reports
`Crediting { topUpId, amount }` and `Delivered`.

**Bank in.** The same, except that the worker reports `Deposit { Bank }` with a documentation IBAN and a reference
first, `/bank` shows those details, and the wait after "I've sent funds" is longer (30 seconds by default).

**Crypto in.** The worker sizes the deposit for the chosen asset (CASH needs no conversion, USDT goes through the PSM,
USDC through the PSM or the pool, PAS through the pool), reports `Deposit { Crypto }` with the session account's address
on network `polkadot`, and watches the account. A short payment is answered with a new `Deposit` for the rest. In demo
mode the session account asks the faucet for a drip after 20 seconds, unless a deposit has started to arrive; a session
that asked for USDT, USDC or CASH switches to the PAS route first, since the faucet gives only PAS. A real transfer from
a wallet is taken either way. After 30 minutes with nothing received the session fails as `Expired`.

**Out, any rail.** `Details`, then `/payout` for where to send the money. The worker calls `payment.request` for the
user's CASH to the reserve account and reports `Collecting { paymentId, amount }`. Once the host reports the payment
complete, the session is released and the worker simulates the payout. A failed payout is refunded: the reserve account
tops the user up by the amount collected, under an id fixed by the session's intent, and the worker reports
`Refunding { topUpId, amount }`. A user who declines the payment ends the session as `Cancelled`, and one without the
balance as `InsufficientBalance`.

**Cancel.** A cancel is honoured while the user has not paid, and the session ends as `Failed { Cancelled }`. Once a
payment is seen the host refuses the cancel itself.

**Restarts.** Each session saves its checkpoint with `fundingProvider.save` before every external action: the phase, the
conversion plan, the pipeline's tick state, the faucet drip it is about to ask for and the top-ups it is about to make.
A save is retried until the host keeps it, since an action taken after a save that did not stick could be repeated. The
host hands the session back on `serveSubscribe` after a restart, and the worker continues from the saved checkpoint.
Every action a resumed step can repeat is safe to repeat:

- Top-up ids are fixed by the session account and the attempt, and the payment id by the session's intent, so the host
  answers a repeated `payment.topUp` or `payment.request` with `AlreadyExists` instead of moving funds twice.
- The faucet drips an address once, so a repeated drip whose first answer was lost is refused on quota, and the session
  waits for the first.
- A report the host already took is refused as `OutOfOrder`, which the worker ignores. An update after the release is
  tried again for a few seconds first, since the host answers the same way before it has marked the session released.
- A waited-for arrival, such as a bank transfer or the payout, has its due time saved, so a restart does not start the
  wait again.

An action the user took on a screen is in product storage and is taken without showing the screen again.

A report the host fails to take is tried again with a growing wait, up to 30 seconds apart, and a chain or network
failure is retried a tick apart. A host refusal and a programming error are not retried. When a failure outlasts its
retries, the worker logs it, drops the session from memory and has it handed out again: after 5 seconds the first time,
doubling each time it stops in a row, up to 10 minutes. The resumed session continues from its checkpoint.

### The faucet

The money a card or bank payment would bring is PAS from the public Paseo faucet,
`POST https://paseo-faucet.parity-testnet.parity.io/drip/web` (source: `paritytech/polkadot-testnet-faucet`). The worker
uses its signed form, which needs no captcha:

```json
{
  "address": "<session account, SS58>",
  "parachain_id": "1500",
  "message": "faucet:<address>:<unix ms>",
  "signature": "0x…"
}
```

The session account signs `message` with its sr25519 key (`<Bytes>`-wrapped, which the faucet accepts along with the raw
form), and the faucet checks the signature against `address` and that the timestamp is under five minutes old. It sends
5000 PAS from Paseo Asset Hub, teleported to Asset Hub Next (para 1500), at most once per 20 hours per address; every
session has a fresh address, so the limit never bites. The answer is NDJSON status lines ending in `{ "hash" }` or
`{ "error" }`, with HTTP 200 either way. `worker/faucet.ts` holds the request and the parsing.

A drip covers a deposit of up to 4990 PAS. The largest amount the provider takes, 5000 CASH, needed about 2200 PAS at
the pool's price when this was written; a deposit above one drip fails with code `above_faucet_drip`. Other failures end
the session with `faucet_refused`, `faucet_timeout` (the PAS did not land within five minutes) or `faucet_blocked`.

The worker calls the faucet with its own `fetch`. A host runs a Worker in a web view with the shared container's network
gates, so a request to another domain needs the product's `Remote` permission for
`paseo-faucet.parity-testnet.parity.io`; the manifest has no field for it. The card and bank screens ask for it with
`permissions.requestRemotePermission` when the user taps Pay or "I've sent funds", since the user is there to answer,
and a grant is kept per product, so it covers the worker. The worker asks again before each drip, which is a no-op once
granted and the only ask a demo crypto deposit makes. The faucet answers with `Access-Control-Allow-Origin: *`.

### How the screens talk to the worker

The worker and the App are the same product, so they share one storage namespace. Before it presents a screen, the
worker writes the session's view under `fundtest:view:<intent>` (and the session id under `fundtest:active:<route>`, for
a host that drops the query string). The screen writes what the user did under `fundtest:action:<intent>`, and the
worker hears it through `localStorage.subscribe`. TrUAPI has no call for a screen to close its own frame, so the worker
does not wait for `presentFrame` to answer: it moves on as soon as the action lands, and the screen asks the user to
close it. A frame closed before any action counts as the user leaving. The keys are in `shared/channel.ts`.

## Reuse of getcash-community

The on-chain leg is getcash's code, not a rewrite. `scripts/fetch-getcash.sh` checks out `paritytech/getcash-community`
at a pinned commit (`ccbe32e`) into `vendor/getcash-community/` and generates its polkadot-api descriptors from the
metadata it commits into `vendor/papi/`. Both are ignored by git and recreated by `npm install`. `tsconfig.json` and
`vite.shared.ts` alias the imports to that checkout:

| Import                      | From getcash          | Used for                                                                   |
| --------------------------- | --------------------- | -------------------------------------------------------------------------- |
| `@getsome/funding`          | `packages/funding`    | `chooseRoute`, `chooseCashTransfer`, `tickOnce`, `sizeNativeBudget`, pools |
| `@getsome/ephemeral`        | `packages/ephemeral`  | session and reserve account keys, the top-up secret layout                 |
| `@getsome/people`           | `packages/people`     | reading the CASH a session account holds on People                         |
| `@getsome/core`             | `packages/core`       | `network.json` (Paseo Next v2 chains) and the token table                  |
| `@getcash/lib/funding-fees` | `lib/funding-fees.ts` | live deposit sizing for each tier                                          |
| `@polkadot-api/descriptors` | `.papi/` metadata     | typed Asset Hub and People APIs                                            |

Two small pieces follow getcash's worker without importing it, since they are plain JavaScript in its worker bundle:
opening a host-routed chain client per step (`worker/src/shared.js`) and the top-up id per attempt
(`worker/src/topup-id.js`).

The checkout is a dependency rather than vendored source because getcash is GPL-3.0-or-later and this repository is MIT:
none of its code is committed here. The worker bundle that `npm run build` produces contains getcash's code and is
therefore a combined work under the GPL.

Chain access goes through `@parity/product-sdk-host` (its host-routed papi provider), whose `@parity/truapi` is
overridden to this repository's build, so the worker holds one TrUAPI client and it has `fundingProvider`.

## Configuration

Configuration is read at build time from `FUNDTEST_*` variables, from the environment or a `.env.local` file in this
directory (ignored by git):

| Variable                     | Default  | Meaning                                                                       |
| ---------------------------- | -------- | ----------------------------------------------------------------------------- |
| `FUNDTEST_DEMO_CRYPTO`       | `1`      | `0` stops demo crypto deposits from the faucet; only a wallet transfer counts |
| `FUNDTEST_CARD_ARRIVAL_MS`   | `5000`   | Wait between Pay and the card payment arriving                                |
| `FUNDTEST_BANK_ARRIVAL_MS`   | `30000`  | Wait between "I've sent funds" and the transfer arriving                      |
| `FUNDTEST_CRYPTO_ARRIVAL_MS` | `20000`  | Wait before a demo crypto deposit is asked for                                |
| `FUNDTEST_PAYOUT_MS`         | `15000`  | Wait between the release and the simulated payout                             |
| `FUNDTEST_BANK_ETA_SECS`     | `172800` | The ETA a bank quote gives, and so how late `.33` on bank arrives             |
| `FUNDTEST_REPORT_RETRY_MS`   | `1000`   | First wait before a report the host failed to take is made again              |

The bundle carries no secret. The worker asks the faucet only when the build targets the Paseo Next v2 testnet
(getcash's `network.json` says `testnet` and the Asset Hub genesis is Paseo Next v2's); otherwise card and bank fail
with code `no_faucet`.

## Build and test

```bash
cd products/fundtest
npm install      # also fetches getcash and generates the descriptors
npm run typecheck
npm test         # pricing, quote ids, triggers, saved state, the faucet request, and resuming after a stop
npm run build    # dist/app and dist/worker
```

`npm install` builds against `js/packages/truapi`, so that package has to be generated and built first
(`./scripts/codegen.sh`, then `npm run build` in `js/packages/truapi`).

## Bundling it into a host

Testnet builds of both apps ship fundtest as the bundled product `fundtest-provider.dot`. Before building the app, run
`hosts/ios/Runscripts/bundle-funding-provider.sh` or `hosts/android/scripts/bundle-funding-provider.sh`. Each builds
this directory from the branch `funding/reference-provider` (or `FUNDTEST_REF`) and copies `worker-manifest.json`,
`dist/worker/index.js` and `dist/app` into the app. An app built without running it has no bundled provider.

## Deploy

Deploying is optional: dev devices use the bundled copy. Deploys are manual; no workflow runs them.

1. Build (above).
2. Sign in to `bulletin-deploy` (`bulletin-deploy login`, with the account that should own the name), then from this
   directory:

   ```bash
   bulletin-deploy ./dist/app fundtest.paseo --js-merkle --env paseo-next-v2
   ```

   `bulletin-deploy.config.ts` describes the App (`dist/app`) and the Worker (`dist/worker`), and the Worker's
   `includes` come from `worker-manifest.json`, so the published manifest and the snapshot a host ships are the same
   file. The name passed must match `domain` in the config byte for byte; set `FUNDTEST_DOTNS_NAME` to deploy under
   another name.

Two things stand in the way today:

- `bulletin-deploy` 0.22.0 validates `includes.funding` as a boolean and rejects the object the core reads
  (`includes.funding must be a boolean when present`). Publishing `"funding": true` instead would not help: the core
  fails to parse that manifest. Until bulletin-deploy accepts the object form, deploy the content with `--content-only`
  and write the `executable` text record of `worker.fundtest.paseo` with the contents of `worker-manifest.json` through
  dotNS directly.
- `fundtest` has an 8-character base. bulletin-deploy's naming notes say older dotNS generations put 6 to 8 characters
  in the band that needs proof of personhood to register. If registration is refused, register it with a
  personhood-holding account or pick a label with a base of 9 or more characters, such as `fundtest-provider`.

### Making phones offer it

The core lists a provider once the host names it in `set_funding_providers`, with the Worker manifest it ships for it.
Give the host the label `fundtest` and `worker-manifest.json` as that snapshot; the core then reads the published
manifest from dotNS and prefers it once it has. Discovering providers from the browse registry is not wired into the
core yet.

### Publishing to browse

browse lists products registered in its Publisher contract. Publishing is done from browse's Developer Portal
(`apps/portal` in `paritytech/browse`, documented in its `docs/publishing-registry.md`):

1. Deploy as above, so `fundtest.paseo` is owned by your account.
2. Open the Developer Portal in a host signed in with the account that owns the name. It lists the names you own.
3. Publish `fundtest`. The portal dry-runs `Publisher.publish(label, proof)` first, then signs it with your root
   account. The call needs a proof of personhood: Lite allows one publish a day, Full five.

This needs the owning account and its personhood, so it is not scripted here.

## Not verified

The chain leg of a card session ran live on Paseo Next v2 outside a host, with the worker's own functions over direct
WebSocket clients: a signed drip to a fresh account (5000 PAS landed on Asset Hub Next, less 0.0006 PAS of teleport
fee), the deposit sizing for 10 CASH (4.25 PAS), the sweep of the surplus, and getcash's pipeline, after which People
held 10.22 CASH claimable by the account. Nothing has run inside a host. In particular:

- the worker's `fetch` reaching the faucet through a host's network gate, and how a host presents the `Remote` prompt
  when a demo crypto deposit asks for it from the headless worker;
- whether a host passes `presentFrame`'s query string through to the App, which the `fundtest:active:<route>` key covers
  if not;
- whether a host prompts for chain access or signing when a worker submits transactions from a session account;
- whether `payment.topUp` with a `PrivateKey` source takes the secret in schnorrkel's layout, which is what getcash
  hands its host;
- that the host replays the latest top-up and payment status on subscribe, which the resume path relies on.
