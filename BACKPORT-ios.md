# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `b29407970a5c9ff791098d9186bef0679f50364a` |
| to | `eb8dc7ce0be9c55d7eac5589d8542c2b9ea090a3` |
| commits | 60 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#450](https://github.com/paritytech/polkadot-ios-community/pull/450)
- [#451](https://github.com/paritytech/polkadot-ios-community/pull/451)
- [#453](https://github.com/paritytech/polkadot-ios-community/pull/453)
- [#455](https://github.com/paritytech/polkadot-ios-community/pull/455)
- [#457](https://github.com/paritytech/polkadot-ios-community/pull/457)
- [#458](https://github.com/paritytech/polkadot-ios-community/pull/458)
- [#468](https://github.com/paritytech/polkadot-ios-community/pull/468)
- [#469](https://github.com/paritytech/polkadot-ios-community/pull/469)
- [#470](https://github.com/paritytech/polkadot-ios-community/pull/470)

<details>
<summary>All 60 commits</summary>

```
7fbb9d595	update: evict occupied statement-store slots with replace_statement_store_account
c255b30f7	Turn off keyboard learning on the recovery phrase field
1a5a57e29	Refuse third-party keyboards while the recovery phrase is typed
a6a001cef	Remove duplicate UIView.create from PolkadotUI
f773713a0	Add CALayer helper that hides content from screen capture
ee32b2719	Protect revealed recovery phrase from screen capture
f4510a64f	Hide recovery phrase input from screen capture and app switcher
398a2932b	Read scene capture state from traits on recovery input
bdc4a27b4	refactoring
a3fb0cb03	Isolate bottom notification presenting to the main actor
27341a3a5	support moving notification max size from constant to parameters
e13f6d987	Merge branch 'develop' into fix/allowance-parameters
2c377eb40	User friendly coinage chat bubble statuses
c6178343d	Ask to open the app to receive a coinage payment in the push banner
aa50ac57f	Merge branch 'fix/allowance-parameters' into feature/replace-statement-store-account
eeef17fd4	make notification max size backward compatible
e5cfa9c10	fix capitalisation
ab9026231	Merge branch 'fix/allowance-parameters' into feature/replace-statement-store-account
2b25b2f0d	backward compatability
15adcd4c3	Confirm before declining a chat request
5d4dd5e7e	fix comments
b1248ad59	Fix comments
8793f734c	Merge branch 'fix/allowance-parameters' into feature/replace-statement-store-account
f9fdd5e78	reuse currency formatter utility
b3d0597c5	drop not needed getSignerKeyId
4b014d290	Add myself as codeowner
34a6287f6	Merge pull request #458 from paritytech/feature/background-claim-registration
7dba385c0	refactoring models
70350d5db	models refactoring
49f6a02ff	Merge pull request #470 from paritytech/codeowners/add-veljkovranic
87d36cc19	drop too detailed comments
40c16f3db	Move the third-party keyboard restriction from RoundedTextView into a policy
4fb977e9f	Merge pull request #468 from paritytech/feature/decline-request-confirmation
97a7fd4df	fix comments
1418092c3	drop not needed maxReplcementAttemps
b1b51108f	Merge pull request #450 from paritytech/fix/recovery-phrase-keyboard
9b24170a1	fix tests
4c0530be7	fix review comments
f5111e975	refactor helpers
d1bf20726	Merge branch 'fix/allowance-parameters' into feature/replace-statement-store-account
3d87282c8	Merge pull request #469 from paritytech/fix/allowance-parameters
ed8360410	Merge branch 'develop' into feature/replace-statement-store-account
e97edadb8	Consolidate screen capture protection into ScreenCaptureProtector
481ab080d	drop backfill
954e4ba9d	fix allocator
3e41d04b1	Merge pull request #457 from paritytech/fix/chat-coinage-simpler-status-messages
62768595b	Merge pull request #451 from paritytech/fix/sensitive-screen-capture
db24888ab	Always report device authentication result and use a fresh context per prompt
2a3007e1f	Ask for device authentication before approving a device pairing
273957608	Ask for device authentication before submitting a transfer
0d1110afc	Ask for device authentication before signing a signing request
28f7b70b8	Ask for device authentication before approving product prompts
36ba20f33	Ask for device authentication before a manual iCloud backup
55f894e50	Narrow Face ID usage description to action confirmations
96c869c56	Docs update
6380c1995	Name the app and list confirmed actions in Face ID usage description
9da3a4611	Reuse one LAContext per device authentication instance
0d706a674	Trim device authentication doc to maintainable facts
1ccc3b649	Merge pull request #453 from paritytech/feature/replace-statement-store-account
eb8dc7ce0	Merge pull request #455 from paritytech/fix/device-auth-gates
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 60 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
