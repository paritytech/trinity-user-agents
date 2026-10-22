# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `06169a99d3d0e0c67a912087280fcf6621706636` |
| to | `e02cb5121da81987cb32e7da0d7377eb37b61229` |
| commits | 67 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#325](https://github.com/paritytech/polkadot-ios-community/pull/325)
- [#340](https://github.com/paritytech/polkadot-ios-community/pull/340)

<details>
<summary>All 67 commits</summary>

```
019976eb8	derivation path definitions
8103fcef3	customizing signers
4cce58412	allocation gating and renew
467f4036e	chat request acceptance
e213a3b90	replace allowance eviction flag with new critical priority
c1e884ae3	refactor allowance cleanup
f316682ae	fix conflicts
272496934	refactor chat acceptance
a894f7b00	refactor allowance
248930578	fix chat activity
5a5596b2b	refactor hop
4aae3c7c4	refactor signer resolver
a29a7903b	optimize funding
59aead7a1	remove not needed cache
9b5b8f036	move chat acceptance out of the chat
628fedb24	fix chat acceptance push notification
01cdae4e6	fix tests
75f223f39	release allowance on account block
f52845f69	move topics to identity account
23c1293b4	statements signing
fc7d5d127	fallback consent
00dd6d0cd	fallback for chat request
246218e50	get rid of unused return value
c494af1d9	fix tests
9761cca85	get rid of PeerSessionAccountsProviding
299303bf6	refactoring
64b229dd0	refactorings signers
16a90a358	database migration
b9bd316f3	statement delivery package
3201c2452	database update
ad12d3f0e	chat request refactoring
c35e7bb31	chat acceptance output
02f55cc66	chat acceptance
e784a8520	fix tests
e1cc75fb4	skip statements anyone can post under a session topic
84336039c	statement renewal background task scheduling
a669f973d	dashboard services manager with headless boot
6438dd664	one coalescing statement renewal run and an awaited recycling pass
b70bc8c59	run statement renewal and recycling from the background task
7595701aa	revert manual coinage recycling
2ebd45ae0	add connection retention
4d29d9ab6	more logs
ee396f647	Merge branch 'develop' into feature/anonymous-chat
a43d50333	fix warning
faa6707e1	fix conflicts
01b7e5f2e	revert breaking change
91f3908b8	Merge branch 'feature/anonymous-chat' into feature/statement-renewal-bgtask
096378b8c	fix comments
af1098a1c	refactoring
a57ac59ba	remove debug tools
ea413f374	refactoring task bootstrapping
2892ccc60	fix race condition
39d7a6cf2	add shared run and cancellation for it
98272366b	fix comment
f312a53b7	refactored zoo of renewers
7abb37f3c	Merge branch 'develop' into feature/anonymous-chat
5fa548b0d	fix ack
596739b2f	fix fallback
6eddf32e2	fix malformed message id
be98c9e51	skip undecodable statements
1a3bb97b4	fix delay
6eeb6499f	clear settings comment
8de1ad1d2	fix conflicts
db820296a	exclude chat acceptance from resending in chat
8bc26a155	Merge branch 'feature/anonymous-chat' into feature/statement-renewal-bgtask
403cad3f7	Merge pull request #340 from paritytech/feature/statement-renewal-bgtask
e02cb5121	Merge pull request #325 from paritytech/feature/anonymous-chat
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 67 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
