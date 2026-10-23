---
title: "Hosts load stacks from descriptors"
owner: "@mordamax"
status: draft
---

# RFC — Hosts load stacks from descriptors

## Summary

Hosts stop keeping chain tables of their own. Each build ships pointers to the stacks it runs, with a bundled copy of
each descriptor from [Trinity stack descriptor](trinity-stack-descriptor.md), and the TrUAPI core resolves, checks and
refreshes them for every host. Polkadot's genesis hashes and the `dot` TLD are reserved, so no stack a build does not
vouch for can present itself as Polkadot. Each build still runs one stack, as today; adding stacks at runtime and
pairing across them are [Running more than one stack](trinity-stack-multiple.md).

## Motivation

Every host keeps its own copy of the facts about the stacks it runs: the mobile apps in Firebase Remote Config,
polkadot-desktop in Firebase and a channel list, dotli in `packages/config/src/network.ts`, and the core in the
truapi-provider catalog and the CLI presets. The [appendix](#appendix-where-todays-firebase-keys-go) shows where each
Firebase key goes. [Trinity stack descriptor](trinity-stack-descriptor.md) lists them. Each wipe makes every copy stale
at once: catching up with one round of previewnet and paseo-next-v2 wipes took 11 files in this repository
([#579](https://github.com/paritytech/trinity-user-agents/pull/579)), and until then `truapi-host dev` declined every
People and Asset Hub request that named the live genesis hash. Hosts also disagree about the same stack: iOS reports its
previewnet build to dapps as network `polkadot`, while dotli reports `previewnet`.

### Requirements

1. **Contained.** Only a host build can introduce a stack that uses Polkadot's genesis hashes or the `dot` TLD, and
   nothing a host keeps for one stack is visible on another.
2. **Uniform.** Every host resolves a given stack to the same chains, runs the same checks and trusts it as far.
3. **Transparent.** Dapps (products, in TrUAPI's terms) see no change to the TrUAPI protocol. `get_chain_info`
   ([RFC 0026](0026-supported-chains.md)) returns the same shape; only the values come from the descriptor.

## Stakeholders

- **Publishers**: Parity for its testnet builds, PCF for its production apps. They configure which stacks a build ships.
- **Host developers**: iOS, Android, polkadot-desktop, dotli and the host CLI. They replace their chain tables with what
  the core resolves.
- **Core developers**: host-rust-core. The core resolves stacks, and truapi-provider accepts light-client chain specs at
  runtime.

## Detailed Design

### Build configuration

A build's configuration is all a publisher changes to ship a host: the pointers of its **Built-in** stacks with a
bundled copy of each descriptor, and which of them may use reserved values. PCF ships its apps by building the same host
code with its own configuration, and Parity's testnet builds do the same with theirs.

| Build                       | Built-in stacks                        |
| --------------------------- | -------------------------------------- |
| Parity's nightly build      | paseo-next-v2                          |
| Parity's preview build      | previewnet                             |
| Parity's mainnet-fork build | polkadot-test, may use reserved values |
| PCF's production build      | Polkadot, may use reserved values      |

- A build takes each pointer from its publisher. Parity's testnet builds take them from the testnets' operators, and
  PCF's production build takes Polkadot's from PCF's own key. The build step fetches and verifies the descriptor and
  bundles the copy, as `truapi-host stack bundle` does: it refuses an unsigned or expired copy and never lowers
  `sequence` below the copy it replaces.
- A Built-in stack MUST be signed, because the host refreshes it unseen.

### Reserved values

The **reserved values** are a short list the core ships: Polkadot's genesis hashes and the `dot` TLD, updated with core
releases. A build can add to the list but never remove from it. Only a Built-in stack its build marks as allowed may
name a reserved value.

- A build pins, from the bundled copy of each stack allowed reserved values, what a refresh must not change: its genesis
  hashes, TLD, `assets`, `contracts`, `coinage` and the relay's light-sync checkpoint. A refresh may move the checkpoint
  forward only when the light client verifies the new one from the pinned one.
- A fork of Polkadot keeps Polkadot's genesis hashes and `dot`, and nothing on the chain tells a host it is a fork
  rather than an impostor. polkadot-test is such a fork, so it runs only where a build ships it and marks it, as
  Parity's mainnet-fork build does.

### Production stacks

A production stack has no single operator. For Polkadot, PCF runs the DUB, deploys the dotNS contracts and publishes the
apps, while RPC nodes, eth-rpc and IPFS gateways come from many providers. The publisher of a host app signs the
Polkadot descriptor its apps ship. Each part is either checked against the chain or pinned by the build:

| Part                                         | Supplied by                                                | Checked or pinned by                                                                       |
| -------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| genesis hashes and the light-sync checkpoint | the chain specs in polkadot-sdk                            | pinned by the build; later checkpoints verified from it                                    |
| bootnodes, RPC, eth-rpc and IPFS endpoints   | open lists that any provider meeting the criteria may join | untrusted: chain reads go through the light client, and content is checked against its CID |
| DUB                                          | PCF                                                        | the attester's allowance on People                                                         |
| dotNS contracts                              | PCF                                                        | discovery from `DotnsGateway.DispatcherAddress`                                            |
| `assets`, `contracts`, `coinage`             | PCF                                                        | pinned by the build                                                                        |
| the choice among the above                   | the publisher, such as PCF for its apps                    | the signature on the descriptor                                                            |

The publisher gains no power it lacks today: it already ships the binary and edits the Firebase values the apps read.
Another publisher may sign its own Polkadot descriptor for its own apps.

### Using a stack

The core resolves stacks for every host. A host gives the core its Built-in pointers, their bundled copies and which of
them may use reserved values, plus a way to fetch bytes. The core gives the host back the resolved stack. The shape of
that interface is the core team's choice. Its decision about a fetched descriptor is already the `trinity-stack` crate's
`accept`: it keeps the stored copy when a fetch fails or breaks a rule, refuses a lower `sequence`, and reports a new
`generation`, staleness and expiry. Using a stack is bound by these rules:

- A host starts from the last descriptor it verified, or from the bundled copy, so a stack never waits on its
  descriptor's server.
- A host does not use a chain or service whose check gives a wrong answer: a genesis hash that differs from the chain or
  its light spec, a TLD that differs from the People chain's `NetworkSuffix.NetworkSuffix`, or a DUB whose attester
  holds no `PeopleLite::AttestationAllowance` entry on People.
- A chain or service that cannot be reached yet is unavailable, not wrong. The rest of the stack keeps working, and the
  host checks the missing part once it connects. A stack is refused only for a wrong answer, never for an unreachable
  one.
- A wrong answer, as after a wipe, makes the host fetch the descriptor again before it refuses the stack.
- A host refreshes the descriptor while it runs and keeps the newer one. A failed fetch only delays a change. A refresh
  may change anything except what the build pins and the stack's TLD, so a stack follows a wipe without a rebuild and
  its accounts never change under it.
- A host also checks that the People chain's `NetworkSuffix.NetworkSuffix` equals the descriptor's `tld`, and that a
  DUB's attester holds a `PeopleLite::AttestationAllowance` entry on People. After a wipe the DUB check may fail until
  the DUB is attested again.
- For a stack that may use reserved values, the checks and every chain read SHOULD go through a verifying light client
  anchored on the pinned checkpoint. RPC endpoints then only carry the light client's traffic, and a hostile one can
  delay answers but not change them. Any other stack may be checked over RPC, which catches staleness but not a hostile
  endpoint.
- The active stack is the only source of chain configuration: the chain set behind `get_chain_info`, the genesis hashes
  and network suffix the signing and pairing roles use, and the TLD accepted in dapp identifiers.
- A host's own features, such as a wallet's chain list, the DUB client and the IPFS gateway, read the same stack as its
  core, so an app never runs one stack while its core runs another.

Nothing a host keeps for one stack is visible to another: stored state, recorded permissions and cached chain data such
as a light client's database and metadata. Two stacks that share genesis hashes, such as a fork and its source, still
keep separate caches, and a new generation of a stack starts its chain caches fresh.

The reserved list replaces today's `DOTNS_TLDS`, and the core accepts the active stack's TLD alongside it. Light-client
hosts need truapi-provider to accept relay and parachain light specs at runtime; today parachain specs come only from
its bundled catalog.

## Drawbacks

- Hosts no longer keep chain tables of their own, and the checks are the core's.
- Hosts gain work: fetching descriptors for the core and showing a stale stack.
- Hosts that run a stack allowed reserved values should run a light client. Desktop and dotli have one; the mobile apps
  use RPC today and would need one for Polkadot.
- Losing a Built-in stack's key without a handover means a release for every build that ships it.

## Security

- **Trust anchors.** The core's reserved list, and the host build: its pointers, which of them may use reserved values,
  and what it pins for those. Everything else is checked against one of those or against the chains.
- **A stolen production key.** It can redirect the RPC nodes, DUB, IPFS gateway and TURN of that publisher's apps on
  their next refresh. It cannot change the chain, the genesis hashes, the TLD, the checkpoint, the payment asset or the
  contracts, which the build pins. Recovery is a release that resets the stack's key and `sequence`.
- **Fork replay.** A transaction signed on a fork of Polkadot can be valid on Polkadot. Only builds that ship such a
  fork can sign one, and they should not hold a production seed.
- **Name shadowing.** Only a stack a build allows reserved values can use `dot`.

## Alternatives Considered

Designs that were considered and not chosen, each with the reason:

- **Reserved values set per build.** Each build would list what it protects, and a build that forgot Polkadot would let
  a stack present itself as Polkadot. The core's list protects every build.
- **One list of Polkadot services on chain, changed by governance.** Every app would use the same services, but changing
  any entry would take a governance vote, and the app that holds the user's keys would no longer choose which DUB and
  IPFS gateway it trusts.

## Future Directions

- **Several publishers' Polkadot as one stack.** Identifying a stack allowed reserved values by its pinned genesis
  hashes, rather than by its key, would let apps from different publishers share state and pair on Polkadot. Nothing
  needs it until a second publisher ships Polkadot.

## Appendix: where today's Firebase keys go

The mobile and desktop apps read the Firebase Remote Config keys below. Each one moves into the descriptor, is read from
the chain, or stays app configuration because it describes the app rather than the stack:

| Firebase key                                                                                                                                                                                             | Read by               | Goes to                                                                                                                                                                                                                                                                  |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `chains_v2`, and `chains` in Android release builds                                                                                                                                                      | iOS, Android, desktop | `chains`: genesis hashes, `rpc` from `nodes`, the relay from `parentId`, and the `hop` service from `externalApi.hop`. Chains outside the stack, such as Polkadot in a testnet build, and wallet fields such as `icon`, `explorers` and `options` stay app configuration |
| `identity_backend_url`                                                                                                                                                                                   | iOS, Android, desktop | the `dub` service                                                                                                                                                                                                                                                        |
| `ipfs_gateway_url`                                                                                                                                                                                       | iOS, Android, desktop | the `ipfs-gateway` service                                                                                                                                                                                                                                               |
| `dot_ns_config`                                                                                                                                                                                          | iOS, Android, desktop | nothing: the contracts are found on chain                                                                                                                                                                                                                                |
| `account_data_store_config`                                                                                                                                                                              | iOS, Android          | `contracts.accountDataStore`                                                                                                                                                                                                                                             |
| `coinage_instance_id`                                                                                                                                                                                    | iOS, Android          | `coinage.instance`                                                                                                                                                                                                                                                       |
| `payment_asset_config`                                                                                                                                                                                   | iOS                   | the asset is `assets` with role `payment`; its label and icons stay app configuration                                                                                                                                                                                    |
| `transaction_extension_versions`                                                                                                                                                                         | iOS, Android          | nothing: read from each chain's metadata                                                                                                                                                                                                                                 |
| `xcm_general_config`, `cross_chain_transfers`                                                                                                                                                            | iOS, Android          | parachain ids come from `chains[].paraId`; asset locations and transfer routes stay app configuration                                                                                                                                                                    |
| `funding_config`                                                                                                                                                                                         | iOS, Android          | stays app configuration; its names live under the stack's TLD                                                                                                                                                                                                            |
| `game_dashboard_url`, `collectibles_enabled`, `collectibles_fallback_url`, `game_results_fallback_url`, `w3s_gate_mode`, `w3s_merchants`, `issue_proxy_url`, `issue_proxy_api_key`, `latest_ios_version` | iOS, Android, desktop | stay app configuration                                                                                                                                                                                                                                                   |
| `funding_domain`, `w3s_start_gate`, `web3summit_config`                                                                                                                                                  | no code               | dropped                                                                                                                                                                                                                                                                  |
