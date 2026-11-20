---
title: "Honour vote"
owner: "@mordamax"
status: draft
---

# RFC: Honour vote

## Summary

Add `honour.vote({ subject, direction })`. The Host shows the user what they are voting on, casts the vote on the People chain with the personhood key and returns only the outcome. The product never receives a proof or an alias, and learns nothing about the user's votes on other subjects. Reading scores needs no new TrUAPI call.

This replaces the per-product proof designs discussed in [PR 1206](https://github.com/paritytech/trinity-user-agents/pull/1206).

## Motivation

Honour (pallet `Honour` on People) gives every subject one public score that all products read. Browse needs a vote button on apps, and so will other products. The pallet's rules:

- Only full people vote. A vote is a `bestow` transaction without a signer, authorized by one ring proof from the personhood key over two contexts: the subject context and the point context.
- A person has one vote per subject. `Votes` is keyed by the person's subject alias.
- A person has 256 points, so at most 256 active votes. A point used less than 24 hours ago can't move to another subject.
- `Points` is keyed by the person's point alias and stores the subject and the direction of the vote that uses it, in clear.

Aliases depend only on the person's key and the context. Honour's contexts are the same for every product: that is how one vote per subject holds across all of them.

### Why the Host casts the vote

PR 1206 has two designs in which the product gets the proof and submits the vote:

- **A.** `account_create_honour_proof`: the product passes the subject, the point and the transaction message, and receives the proof and both aliases.
- **B.** A generic multi-context `createAccountProof`, with Honour's contexts moved under a product `honor.<tld>`. A product then needs a grant from `peopl` for the key and a new grant from `honor` for its contexts.

Both give the product the proof and the aliases. That causes four problems, and none of them depends on how the call is named or permissioned.

**1. The product can read every vote the user ever cast.** To vote, the caller must choose a free point. It can only find one by deriving the user's 256 point aliases and reading `Points` with them, and each used point returns its subject and direction. A product that can vote can therefore list the user's votes from every product and keep watching them. In A, the product also receives a point alias with every proof, for any point it asks for.

**2. The product gets an identifier that is the same in every product.** Product-scoped contexts (RFC-0004, RFC-0024) give each product different aliases for the same person, so products can't match their users. Honour's contexts are shared on purpose, under `honor.<tld>` too. Every product that obtains a user's Honour aliases obtains the same 256 values and can join its records with another product's on them.

**3. The product can vote without the user.** RFC-0024 treats a proof as a bearer token over an opaque message, and forbids a user prompt for it: the allowlist is the only gate. Once a product is allowed, it can cast votes on any subject in the user's name, and use all 256 points so that the user can't vote for 24 hours. Honour's score has value only if each vote is a different person's own decision. A product with many users could produce many votes.

**4. A prompt can't fix 3 while the product supplies the message.** The direction of a vote is in the call, not in the contexts. A sheet that says "Up on chess.dot" can't check that the message it proves is not "Down on chess.dot". The Host can ask for meaningful consent only if it builds or decodes the call it proves, and design A changed to do that still has problems 1 and 2.

B also needs changes that A and this RFC don't:

- The pallet's contexts change. The chain can't map existing aliases to new ones by itself, because aliases come from the person's secret key. Each person has to re-prove their votes, or they are dropped.
- TrUAPI gains a multi-context proof call, or `createAccountProof` changes on the wire to take several contexts.
- `peopl` grants `context` to every voting product. The manifest RFC says that this grant lets the grantee sign as `peopl`, which is far wider than voting.
- The manifest gains a new scope or a public-context policy for `honor`.

When the Host casts the vote, the aliases never leave the device that holds the key. Every vote passes a sheet that the Host renders from the call it signs, and products learn only the outcome. The chain still enforces one vote per person per subject, and the Host doesn't add rules of its own. No runtime or manifest change is needed.

The core proves under the contexts the pallet defines and never exposes them through TrUAPI. RFC-0024 requires product-owned contexts for proofs that products request, and doesn't cover proofs the Host makes for itself. If Honour later moves to product-owned contexts, only the core changes. The core already works this way for PGAS and statement-store allowances: it picks a slot, proves, submits a transaction without a signer and checks the result.

### Tested

Votes cast the way this RFC describes, with the `verifiable` revision the core pins, were finalized on a test network, and the chain rejected a second vote on the same subject.

- Finding a free point costs two storage requests.
- The proof is 817 bytes and takes about 62 ms over a full ring, the same as a single-context proof.
- Scores for 1000 subjects come back in one `chainHead_v1_storage` request.

## Approach

```ts
const result = await truapi.honour.vote({ subject: "dotns:chess.dot", direction: "Up" }); // or "Down"
// "Voted" | "Declined" | "AlreadyVoted" | "NoPointAvailable" | "NotEligible" | "Failed"
```

- `subject` is a string of at most 128 bytes, as shown to the user. The subject id on chain is `blake2_256(subject)`. The Host hashes the string as given and doesn't normalise case or Unicode form, so products that mean the same subject must send the same string.
- The Host on the device that holds the personhood key derives the subject alias and the 256 point aliases (the point aliases can be cached), and reads `Votes` and `Points` with them.
- It shows a sheet with the subject and the direction. If no point is free, the sheet lists the user's votes whose points can move, and the user picks one.
- It builds `bestow`, proves over both contexts, submits the transaction and waits for finalization. It runs one vote at a time per person, so two products can't take the same free point.
- The product receives:
  - `Voted` once the vote is finalized;
  - `Declined` if the user declines;
  - `AlreadyVoted` if the user has an active vote on the subject;
  - `NoPointAvailable` if every point was used in the last 24 hours;
  - `NotEligible` if the user isn't a full person. Lite people can't vote;
  - `Failed` if the chain didn't include the vote, for example because the ring changed or the node was unreachable.
- Every outcome comes after the user has seen the sheet, which states it, so a product can't probe the user's votes or personhood by calling `vote` repeatedly.
- A paired Host forwards the request to the device with the key, the same way it forwards `resourceAllocation.request`. A Host without the key, or without Honour on its People chain, fails with `Unsupported`.

Scores are public chain state. Products read `Honour.Tally` by subject id through the chain API. A missing entry means a score of -1, the pallet's default. The product SDK gains a reader that fetches a page of subjects in one request, instead of one request per subject.

## Trade-offs

- The core reads Honour storage and builds Honour transactions, so a breaking pallet change needs a Host release. PGAS and statement-store allowances have the same dependency.
- Products can't build their own vote flow, such as voting on several subjects in one action. Each vote needs one confirmation.
- `AlreadyVoted` tells the product that the user has voted on that subject, possibly in another product. The user sees this on the sheet first.
- A product that requested a vote knows its subject and its time. It can find the transaction and the point alias that the transaction wrote. When that point later moves to another subject, the product can link the two votes. Anyone watching `Points` can link the votes that reuse a point, without knowing whose they are. Closing this needs a pallet change.
- A user can't change direction on a subject in one step, because the pallet rejects a second vote on a subject the user has an active vote on. Changing that is a runtime decision outside this RFC.
- Rejected: designs A and B from PR 1206, for problems 1 to 4 above.

## Alternatives

Both alternatives move Honour's logic from the core to a product `honor.<tld>`. Products call `honour.vote({ subject, direction })` in each of them, so the choice changes who implements the call and doesn't change the products that use it.

The choice matters beyond Honour. Other protocols that allow one action per person, such as polls or quotas, raise the same question. Under this RFC each of them adds code to the core and needs a Host release.

### Alternative 1: Honour as a product

`honor.<tld>` casts votes in its own UI, and other products send the user there. This needs:

- Honour's contexts move under `honor.<tld>`. Existing votes are re-proved or dropped.
- TrUAPI gains a multi-context proof call.
- `peopl` grants `context` to `honor`.
- `honor` gets the permission to submit transactions.

What it costs in privacy:

- honor derives every user's aliases, so it can read each user's full vote history (problem 1). No other product gets the aliases, so problem 2 is limited to honor.
- RFC-0024 doesn't allow a prompt for proofs, so honor can vote for any user without the user (problem 3). Its vote sheet is its own code.
- A product that can submit transactions can publish any data in them, including the vote history. An HTTP permission would let it send the history to a server.
- Trust moves from the Host to honor's bundle and the owner of the `honor` dotNS name. One bundle update or a name transfer changes the code that runs for every user.

What it gains:

- Honour changes ship without a Host release.
- honor can offer flows the Host sheet doesn't, such as a list of the user's votes or a vote on several subjects in one action.
- The core doesn't contain Honour code.

### Alternative 2: Honour as a provider worker

This follows the Funding modality: the core keeps what must not depend on the provider, and the provider's Worker does the steps specific to its service. The core forwards `honour.vote` to honor's Worker.

| The core | honor's Worker |
|---|---|
| Renders the sheet from the decoded call | Picks the point, or lists the votes whose points can move |
| Proves under honor's contexts, only inside this flow | Builds the `bestow` call |
| Submits only that call | Shows the user's votes |
| Returns only the outcome to the requesting product | Follows pallet changes |

Its privacy is the same as this RFC's only if all of these hold:

- honor doesn't get a proof grant from `peopl`. It gets proofs only through this flow, so every vote passes the core's sheet.
- honor doesn't get remote permissions, and only the core submits. The vote history the Worker reads stays on the device.
- The core decodes the call before it proves it, so the sheet shows what the core proves. The core still needs the call layout and the mapping from `subject` to the string it shows.
- The requesting product receives only the outcome.

If one of them doesn't hold, the privacy is that of alternative 1. When all hold, the difference from this RFC is that honor's code reads each user's vote history on that user's device. It can't send the history anywhere or cast a vote the user didn't see on the sheet.

This needs everything alternative 1 needs, except the `context` grant and the submit permission, and also:

- a new modality: a Worker trait and the core's path to invoke it;
- a grant that lets a product prove only through that modality;
- call decoding for the sheet in the core.

What it gains:

- A second protocol of this kind adds a Worker instead of core code and a Host release. This is the strongest argument for it.
- Honour changes ship without a Host release, unless they change the call layout the sheet decodes. A sheet that decodes calls from runtime metadata removes that exception and the per-protocol code in the core, but shows raw call fields to the user.

### Comparison

| | This RFC | Alternative 1 | Alternative 2 |
|---|---|---|---|
| Reads a user's votes | the core, on the device | honor, for every user, and can publish them | honor's Worker, on the device, and can't send them |
| Can vote without the user | no one | honor | no one |
| Changes outside the Host | none | contexts, proof call, `peopl` grant | contexts, proof call, modality, grant |
| Breaking Honour change | Host release | honor release | honor release, Host release if the call layout changes |
| Next protocol of this kind | core code and a Host release | another product with the same privacy cost | another Worker |

This RFC needs the least change and has the strongest privacy, so it is the proposal for Honour now. Alternative 2 is the option to revisit when a second protocol of this kind comes up. Products don't change if Honour moves to it later.

