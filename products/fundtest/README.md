# fundtest: reference funding provider for Paseo Next v2

`fundtest` is a TrUAPI product that serves the funding modality on the Paseo Next v2 testnet, so the CASH card and the
funding overlay can be run end to end on a phone. It simulates only the off-chain leg (the card charge, the bank
transfer, the payout). Everything on chain is real: the deposit lands on a per-session account on Asset Hub, getcash's
pipeline converts it to CASH and sends it to People, and `payment.topUp` claims it into the user's balance.

The product has two executables:

- **Worker** (`worker/`): serves the `FundingProvider` trait. It answers quote asks, runs each assigned session and
  reports its progress.
- **App** (`app/`): the provider's own screens, which the worker shows with `presentFrame`: `/card` (card form and Pay),
  `/bank` (bank details and "I've sent funds") and `/payout` (where an outbound payout goes).

The dotNS label is `fundtest`, live as `fundtest.paseo`.

## What is real and what is simulated

| Step                                          | Real or simulated                                                       |
| --------------------------------------------- | ----------------------------------------------------------------------- |
| Quotes                                        | Simulated: fixed prices (below), answered from the worker               |
| Card charge, bank transfer                    | Simulated: a screen and a wait                                          |
| The deposit reaching Asset Hub                | Real transfer from the provider's faucet account to the session account |
| A crypto deposit                              | Real: from the user's wallet, or from the faucet in demo mode           |
| Conversion on Asset Hub, transfer to People   | Real: getcash's pipeline, one transaction signed by the session account |
| Crediting the user                            | Real: `payment.topUp` with the session account's key                    |
| Collecting an outbound payment                | Real: `payment.request` to the provider's collection account            |
| Paying the user out after an outbound session | Simulated: a wait; the CASH stays with the collection account           |

Prices are fixed so a run is predictable: 1 CASH costs 0.99 EUR, the provider takes 1%, and each rail adds a fixed
network fee (0.10 EUR on card and bank, a small amount of the asset on crypto). Crypto assets are priced at 1 CASH per
USDT, USDC and CASH, and 4 CASH per PAS. A quote holds for 60 seconds and gives an ETA by rail: 5 minutes for card, 2
days for bank, 10 minutes for crypto. Amounts below 10 CASH or above 5000 CASH are refused with `BelowMinimum` or
`AboveMaximum`, and card and bank are refused with `CountryUnsupported` outside the countries the manifest lists (the
United States is left out on purpose). The deposit a crypto session asks for is not the quote's figure: it is sized live
from the chain so the conversion lands the full amount.

### Picking a failure case

The cents of the amount choose what goes wrong:

| Cents | Inbound                                                         | Outbound                           |
| ----- | --------------------------------------------------------------- | ---------------------------------- |
| `.13` | The payment is seen, then refunded: `Failed`, code `refunded`   | The payout fails after the release |
| `.23` | The credit is split over two top-ups, so the host shows a retry | No change                          |
| `.33` | The payment arrives only after the quoted ETA has passed        | The payout arrives after the ETA   |

A crypto deposit made from a wallet ignores `.13`: the refund case only runs when the faucet would make the deposit.

## How each rail runs

Every session first derives its own account on Asset Hub from host entropy under this product, with the label
`onramp:eph:fundtest:<intent>`, the way getcash derives its burners. The host derives the same account for the same
user, product and label, so the account survives a restart and its secret is never stored.

**Card in.** `AwaitingPayment` and `Details`, then `/card`. After Pay, a short wait stands in for the card network and
the session reports `PaymentReceived { finalized: false }`. The worker then picks the route the way getcash's fiat rails
do (USDT through the PSM when it can serve, PAS through the pool otherwise), sizes the deposit, and has the faucet send
it to the session account. Once that transfer is final it reports `PaymentReceived { finalized: true }` and
`Converting`, runs the conversion and the transfer to People, claims the CASH with `payment.topUp`, and reports
`Crediting { topUpId, amount }` and `Delivered`.

**Bank in.** The same, except that the worker reports `Deposit { Bank }` with a documentation IBAN and a reference
first, `/bank` shows those details, and the wait after "I've sent funds" is longer (30 seconds by default).

**Crypto in.** The worker sizes the deposit for the chosen asset (CASH needs no conversion, USDT goes through the PSM,
USDC through the PSM or the pool, PAS through the pool), reports `Deposit { Crypto }` with the session account's address
on network `polkadot`, and watches the account. A short payment is answered with a new `Deposit` for the rest. In demo
mode the faucet makes the deposit after 20 seconds; a real transfer from a wallet is taken either way. After 30 minutes
with nothing received the session fails as `Expired`.

**Out, any rail.** `Details`, then `/payout` for where to send the money. The worker calls `payment.request` for the
user's CASH to the collection account (the faucet account, so outbound CASH refills it) and reports
`Collecting { paymentId, amount }`. Once the host reports the payment complete, the session is released and the worker
simulates the payout. A user who declines the payment ends the session as `Cancelled`, and one without the balance as
`InsufficientBalance`.

**Cancel.** A cancel is honoured while the user has not paid, and the session ends as `Failed { Cancelled }`. Once a
payment is seen the host refuses the cancel itself.

**Restarts.** Each session saves its phase, its conversion plan, the pipeline's tick state and the top-ups it made with
`fundingProvider.save`. The host hands the session back on `serveSubscribe` after a restart, and the worker continues
from the saved phase. An action the user took on a screen is in product storage and is taken without showing the screen
again.

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
| `@getsome/ephemeral`        | `packages/ephemeral`  | session account and faucet keys, the top-up secret layout                  |
| `@getsome/people`           | `packages/people`     | reading the CASH a session account holds on People                         |
| `@getsome/core`             | `packages/core`       | `network.json` (Paseo Next v2 chains) and the token table                  |
| `@getcash/lib/funding-fees` | `lib/funding-fees.ts` | live deposit sizing for each tier                                          |
| `@polkadot-api/descriptors` | `.papi/` metadata     | typed Asset Hub and People APIs                                            |

Three small pieces follow getcash's worker without importing it, since they are plain JavaScript in its worker bundle:
opening a host-routed chain client per step (`worker/src/shared.js`), the top-up id per attempt
(`worker/src/topup-id.js`), and the faucet transfer with the `min_balance` raise (`lib/faucet.ts`).

The checkout is a dependency rather than vendored source because getcash is GPL-3.0-or-later and this repository is MIT:
none of its code is committed here. The worker bundle that `npm run build` produces contains getcash's code and is
therefore a combined work under the GPL.

Chain access goes through `@parity/product-sdk-host` (its host-routed papi provider), whose `@parity/truapi` is
overridden to this repository's build, so the worker holds one TrUAPI client and it has `fundingProvider`.

## Configuration

Configuration is read at build time from `FUNDTEST_*` variables, from the environment or a `.env.local` file in this
directory (ignored by git):

| Variable                     | Default | Meaning                                                                         |
| ---------------------------- | ------- | ------------------------------------------------------------------------------- |
| `FUNDTEST_FAUCET_SEED`       | none    | Mnemonic or 0x-prefixed 32-byte entropy of the faucet account                   |
| `FUNDTEST_DEMO_CRYPTO`       | `1`     | `0` stops the faucet making crypto deposits; only a wallet transfer then counts |
| `FUNDTEST_CARD_ARRIVAL_MS`   | `5000`  | Wait between Pay and the card payment arriving                                  |
| `FUNDTEST_BANK_ARRIVAL_MS`   | `30000` | Wait between "I've sent funds" and the transfer arriving                        |
| `FUNDTEST_CRYPTO_ARRIVAL_MS` | `20000` | Wait before the faucet makes a demo crypto deposit                              |
| `FUNDTEST_PAYOUT_MS`         | `15000` | Wait between the release and the simulated payout                               |

The faucet seed is compiled into the worker bundle, so anyone who fetches the bundle can take the faucet's funds. Use a
dedicated testnet account holding only what the tests need. The worker refuses to use the faucet unless the build
targets the Paseo Next v2 testnet (getcash's `network.json` says `testnet` and the Asset Hub genesis is Paseo Next
v2's). The faucet needs PAS, USDT, USDC and CASH on Paseo Next v2 Asset Hub: card and bank deposits are made in USDT or
PAS, whichever route the chain offers, and crypto deposits in the asset the user picked. Without a faucet seed the card
and bank rails fail with code `no_faucet`, and crypto waits for a wallet transfer.

## Build and test

```bash
cd products/fundtest
npm install      # also fetches getcash and generates the descriptors
npm run typecheck
npm test         # pricing, quote ids, triggers and saved state
FUNDTEST_FAUCET_SEED="…" npm run build   # dist/app and dist/worker
```

`npm install` builds against `js/packages/truapi`, so that package has to be generated and built first
(`./scripts/codegen.sh`, then `npm run build` in `js/packages/truapi`).

## Deploy

Deploys are manual; no workflow runs them.

1. Build with the faucet seed set (above).
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

## Waiting on core changes

These are marked `TODO(core)` in the code:

- **`networks` on the crypto route.** `worker-manifest.json` already declares `"networks": ["polkadot"]`; this tree's
  core ignores the field until the parallel change lands. The `Deposit { Crypto }` report names the network `polkadot`.
- **`FundingFailure::Refunded`.** Until it exists here, a refund is reported as `Other { code: "refunded" }`
  (`worker/session.ts`).
- **The payout update.** `FundingUpdate::Payout { outcome }` after `Released` is not in this tree yet; `reportPayout` in
  `worker/session.ts` logs the outcome instead of reporting it.

## Not verified

Nothing here has run against a host or a chain. In particular:

- the deposit sizing, conversion and transfer to People, which are getcash's code but driven by this worker;
- whether a host passes `presentFrame`'s query string through to the App, which the `fundtest:active:<route>` key covers
  if not;
- whether a host prompts for chain access or signing when a worker submits transactions from a session account;
- whether `payment.topUp` with a `PrivateKey` source takes the secret in schnorrkel's layout, which is what getcash
  hands its host;
- that the host replays the latest top-up and payment status on subscribe, which the resume path relies on.
