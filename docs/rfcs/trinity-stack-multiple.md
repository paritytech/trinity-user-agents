---
title: "Running more than one stack"
owner: "@mordamax"
status: draft
---

# RFC — Running more than one stack

## Summary

A person adds a stack to a host at runtime by scanning a QR code or opening a link that carries a pointer from
[Trinity stack descriptor](trinity-stack-descriptor.md). The stack runs as an **Imported** stack: trusted less than the
stacks the build ships, never allowed reserved values, and only in builds that accept imports. This lets a tester reach
a stack that did not exist when their app was built, such as a two-hour fork from the Product Preview Network (PPN).

[Hosts load stacks from descriptors](trinity-stack-hosts.md) keeps one stack per build. Once a host can hold more than
one, two things follow, and this RFC covers both: each stack's data must live apart, and paired hosts must agree on
which stack a session runs. For pairing, this RFC states the requirements and the problems a design has to solve, and
leaves the design to the host teams.

## Motivation

The PPN spawner brings up a fork for two hours, and no phone can reach it: iOS and Android map their build
configurations onto three fixed stacks. pop-e2e reaches it only through `FORK_*` variables filled in by hand.

A desktop or web host runs its own stack while it is not paired: the one its build ships, or one a person picked or
imported. Dapps run on it as they do today. Pairing puts the phone in charge of signing, and the two hosts talk over a
People chain. A transaction signed for one stack's genesis hash is useless on another, and the hosts cannot talk over
two different People chains, so a session on two stacks cannot work.

### Requirements

1. **Runtime.** A host reaches a stack that did not exist when the host was built. PPN brings up a fork for two hours
   under a generated domain, and a tester scans its QR code with the Polkadot app they already have installed.
2. **Contained.** A QR code cannot make a host treat someone's chains as Polkadot, and nothing granted or stored on an
   imported stack is visible on another.
3. **No data loss.** Adding or switching stacks, pairing and unpairing never overwrite what a person keeps on another
   stack.
4. **Shared.** Paired hosts run the same stack for the whole session.
5. **The phone decides.** When paired hosts' stacks differ, the phone has the final say, because it holds the person's
   keys. Neither host runs a stack its own build refuses.

## Stakeholders

- **Host developers**: iOS, Android, polkadot-desktop and dotli. They add a way to import a stack, pick one and see
  which one they are on, keep each stack's data apart, and agree on a stack when paired.
- **Stack operators**: PPN, whose spawner shows each instance's link and QR code, which `truapi-host stack link` and
  `encodeLink` in `@parity/trinity-stack` produce.
- **Host specification maintainers**: the `stack` URL route, and stack information in the pairing messages.

## Detailed Design

### Links

A link or QR code carries one pointer, in the format [Trinity stack descriptor](trinity-stack-descriptor.md) defines.
The `stack` route joins `pair` in the host specification's URL schemes.

### The Imported tier

- A build chooses whether it accepts Imported stacks. PCF's production build does not; Parity's testnet builds do, in
  dev mode.
- An Imported stack may never use reserved values. A fork of Polkadot runs only where a build ships it.
- A host activates an Imported stack only after the person confirms it. The prompt shows the stack's name and that it is
  imported, because the name is whatever the descriptor calls itself. It also says when the stack is local and
  unencrypted, or when it uses the same TLD as a stack the build ships and so shares its accounts.
- A host marks an Imported stack as imported wherever it asks the person to approve something.
- A pointer that identifies a stack the host already holds takes that stack's tier, so a link to a Built-in stack opens
  it with no import prompt.
- An Imported stack has no stored copy, so the host fetches it before first use. The checks and refresh rules of
  [Hosts load stacks from descriptors](trinity-stack-hosts.md) apply to it as to any stack.
- After `expiresAt`, a host stops offering the stack. If it is the active stack, the host tells the person and lets them
  finish or switch rather than cutting them off. The host may then delete what it kept for that stack, telling the
  person first if any of it is not on chain. The device clock is enough, because expiry grants no trust.

### Per-stack storage

Importing has a prerequisite on mobile. Today each app build serves one network and keeps one set of user data: Android
stores it in `app_v2.db`, and iOS in `UserDataModel_v3.sqlite` and `SubstrateDataModel_v3.sqlite`. Switching stacks over
that storage would overwrite the person's data. A host that offers more than one stack must first keep each stack's data
and sessions apart, so switching never loses anything. How it does that is the host team's choice.

### Pairing

A pairing design has to solve these problems:

- telling the other host which stack each side runs, which needs a change to the pairing messages in the host
  specification;
- agreeing on one stack when they differ, within the requirements above;
- refreshing a stale copy when both hosts hold the same stack and one has an older descriptor;
- keeping sessions on different stacks apart when a phone is paired with several hosts;
- which stack a session runs on after either host restarts, and which stack each host returns to when it ends;
- what each host shows when they cannot agree.

Each host fetches and checks a descriptor itself, as [Hosts load stacks from descriptors](trinity-stack-hosts.md)
requires. Neither takes a descriptor's contents on the other's word. How hosts solve the rest is the host teams' choice.

## Drawbacks

- Mobile hosts need per-stack storage before they can offer imports.
- Hosts gain an import prompt, a stack picker and an imported badge on approval prompts.
- Pairing needs a change to the host specification's pairing messages, which every host implements.

## Security

- **What an Imported stack sees.** Whoever makes an import link chooses the stack's services, so anything a host sends
  them, such as attestation requests to its DUB or addresses to its RPC nodes, reaches that person. The prompt and the
  badge exist so the person knows which stack they are on.
- **Shared accounts.** Accounts derive from the seed and the TLD, so an Imported stack that reuses a testnet's TLD sees
  the same accounts as that testnet. A signature not bound to a genesis hash, such as a raw message or a statement, is
  valid on both, and the stack's operator can link the person's activity on it to their testnet account. The risk stays
  inside one build, because builds that accept imports should never hold a production seed.
- **Local stacks.** A stack on the local network is unencrypted, so someone on the same network could serve a different
  one. That is acceptable for dev builds and test accounts, and a signed local stack avoids it.
- **Pairing proposals.** A pairing request shown by any page can name a stack. A stack one side does not hold goes
  through the same rules and prompt as an import, and the phone, not the page, decides which stack the session runs.

## Future Directions

- **A signed registry of testnets.** One build could offer many testnets in its stack picker without a release and
  without a QR code for each. It would add a registry, a key to guard and a third tier between Built-in and Imported.
