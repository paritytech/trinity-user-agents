# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `9ec050eb13791ae559fec732fda016b4d0d25abb` |
| to | `3ebf9ed99b96d0a1ceabc8a00f031c378d3d638b` |
| commits | 56 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#343](https://github.com/paritytech/polkadot-ios-community/pull/343)
- [#474](https://github.com/paritytech/polkadot-ios-community/pull/474)
- [#476](https://github.com/paritytech/polkadot-ios-community/pull/476)
- [#477](https://github.com/paritytech/polkadot-ios-community/pull/477)

<details>
<summary>All 56 commits</summary>

```
fbcdee7d4	Declare non-terminal transfer states explicitly per direction
ca69ea083	Schedule durable coinage transactions in a write of their own
b6e0a0796	Mark an outgoing payment unclaimed once its claim window closes
0cdb43545	Show the recipient that an unclaimed payment's claim period expired
bf0ddad3d	Let the sender reclaim a payment's unclaimed coins after the claim window
e217fa3ec	Show the claim period as expired only once the recipient's claim gives up
f2edee440	Polish the reclaim bubble: wider button padding, amount shimmer, clockwise spin
63dd54e47	Fix SwiftLint warnings from the reclaim changes
0dabd265f	Merge branch 'develop' into feature/reclaim-funds
80107eaef	Use FixedDateProvider in reclaim coins service tests
ab7b42e21	Remove the unused absent claim detection
e5639a58a	Fetch reclaimable coins with a keyed query instead of a snapshot stream
cbab4f045	Clarify why a retried reclaim input is not failed
884bf53b5	Share the registration hook and error mapping across coinage tx entry points
a43cd0869	Track in-flight reclaims in the chat interactor
0ae28affc	Name the reclaim rebuild test after the ledger's own read-back
e749bd7bd	Document why the window close is a stream
60993901a	Extract the amount shimmer into a reusable ShimmerMask
7b7bcccd4	Move the reclaim button into its own transfer message component
0c26da0df	Merge branch 'develop' into feature/reclaim-funds
30ed0c0e4	Merge branch 'develop' into feature/reclaim-funds
d9e31beab	privacy mode bullet points
63cb4dbeb	clean ups
5c3394c77	address codereview
1e0d75269	fix conflicts
eef95d49b	Persist the ring unloaded count on vouchers
3c1c16104	Release clearing vouchers by ring fill or anonymity-minutes
a6b41da7d	Read voucher release thresholds from coinage_privacy_config
83bf1357b	Make Balanced the default privacy mode
4d0b9c71a	Await chain status anchor probes without a deadline and drive the timeout with an injected clock
1651a4530	Fix Sendable and complexity warnings in privacy config store and voucher location mapper
8151358ab	Address review: inline illustration, drop wireframe guard, reword comment
3432d7b0b	Share one cancellation-aware test gate between the anchor provider mock and the activity stub
a31bd4c72	Merge remote-tracking branch 'origin/develop' into feature/reclaim-funds
c452e965a	optimize tests
8640f66cb	fix README
93b317fd2	Merge pull request #477 from paritytech/fix/chain-status-probe-tests
0c18a6c84	Merge pull request #476 from paritytech/feature/clearing-voucher-release
e5097f667	Merge branch 'develop' into feature/reclaim-funds
3cea8e5dd	refactor time stream
4c20ce55c	refactor reclaim failure status
3f1f6c98d	disable flaky test
0d14c433c	refactor reclaim gate
19bb974b1	refactor tests
8217ab6e1	refactor reclaim status
a1f2f2fbe	fix tests
0742a0622	authorize reclaim
14757df26	refactoring typography
af1ca947a	introduce persistent reclaimed state
e114ba317	refactor tests
6917a4f56	fix tests
4e6ff3a25	undo resolution
e12aeaf42	fix comments
929ac175e	drop dead code
41ce61a71	Merge pull request #474 from paritytech/feature/privacy-mode-descriptions
3ebf9ed99	Merge pull request #343 from paritytech/feature/reclaim-funds
```

</details>

## How to finish this

You are completing this pull request. Work on its branch and push to it.

1. Run the refresh. Do not merge the trees by hand:

   ```bash
   scripts/refresh-host-import.sh refresh ios
   ```

   It replaces `hosts/ios` with the source's tree, re-applies this repository's adaptations as a three-way patch, and compares every path against the source by blob hash in both directions. That comparison is the point: it is what catches upstream work silently dropped and adaptations that no longer apply. A hand-merge produces a plausible tree and none of those checks.

2. Read what it reports before committing.

   - A conflict is left unmerged on purpose, so git refuses to commit it. Resolve each one, keeping this repository's adaptation unless the source has clearly superseded it.
   - `adaptation left no difference` means either the source adopted that change or it did not apply. Check which, and say so in the pull request.
   - If the script refuses the result outright, do not force it. Recover with `git reset --hard HEAD` and say what happened.

3. Build what the change touches. An upstream change that adds a requirement to a protocol will not show up as a conflict, because the comparison reads content and not types; it shows up as a conformer in this repository that no longer compiles.

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 56 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
