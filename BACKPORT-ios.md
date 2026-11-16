# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `0fbe16716677bb6df5aef07f21385d682d07beaf` |
| to | `7f3639bd11f663dd2b1c4fa8b04186d71d229ec0` |
| commits | 47 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#366](https://github.com/paritytech/polkadot-ios-community/pull/366)
- [#383](https://github.com/paritytech/polkadot-ios-community/pull/383)
- [#421](https://github.com/paritytech/polkadot-ios-community/pull/421)
- [#426](https://github.com/paritytech/polkadot-ios-community/pull/426)
- [#442](https://github.com/paritytech/polkadot-ios-community/pull/442)

<details>
<summary>All 47 commits</summary>

```
1f3a02113	Send and show chat seen watermark
2db2ae17c	Send seen watermark only on advance, debounced, marked after delivery
f703031c2	Gate seen watermark to accepted unblocked person chats
9c2edc1d8	Resolve unknown seen watermark id by timestamp
54bb16ed9	Exclude protocol events from seen watermark
292ece50d	Compare seen watermark message ids in canonical lowercase
792603d8c	Fix SwiftLint parameter count and function chain warnings
2fad29129	Show single checkmark for sent and delivered, double for seen
19ba9f64c	Apply chat request cancel on the receiver
d75a9e19f	Remove an accepted empty chat when its request is cancelled
a96d0a69a	Cancel an outgoing chat request from the sender
ea7755b02	Add a cancel button to the outgoing chat request banner
4178bebe5	Match the outgoing request footer background to the incoming one
9743ef1e2	Show a cancelled footer when the sender cancels an open incoming request
565e9e9a6	Hide an outgoing chat request while its cancel runs
bbcdac4ba	Use design system spacings in the chat request UI
2bda179e9	Align seen receipts with RFC 0006 review
aa076ad67	Count seen receipt content by explicit type and fetch newest delivered row
6736393aa	Share one seen receipt service per chat flow and simplify receipt lookups
09f31dffb	Move ChatPresenter delegate conformances to separate file
db3a96b32	Bind the chat request cancel proof to the request id
da7523e2f	Sign chat request cancels with the current period's one-shot account and a base-spec expiry
c745d9d3f	Share message exchange queue test doubles
5f19672eb	Ignore an identical seen receipt while one is pending
406462637	Replace a pending seen receipt with a newer one
b69a66130	Cancel each live chat request copy from its own account instead of claiming a slot
913d87728	Keep seen receipts out of compacted batches
1f1acccda	Retry a chat request cancel on transient failures with backoff
005993836	Decide chat request cancel retries with an Error extension
af92b009d	Make chat request cancel best effort and keep the cancelled request row
b7ff07ca5	Merge pull request #366 from paritytech/feature/chat-seen-watermark
95dda832f	Retry chat request cancels in the background and stop retired first deliveries at once
331a889c6	Video/voice call chat bubble UI fixes
b9801a727	test cleanup
568b13bd1	Merge remote-tracking branch 'origin/develop' into feature/chat-request-cancellation
144ae01f6	Merge remote-tracking branch 'origin/develop' into feature/release-improvements
a276372c5	Merge branch 'feature/release-improvements' into feature/chat-request-cancellation
fbc503536	prefer behavior tests over deep testing
0001ea408	Merge pull request #421 from paritytech/fix/call-no-answer-label
5bf806287	Merge branch 'develop' into chore/testing-improvements
3e33d57a7	Merge pull request #426 from paritytech/chore/testing-improvements
db8668d25	Fix build breaks from the develop merge
44fee0d9e	Keep a blocked contact when its chat request is cancelled
048174681	Save the removed-chat tombstone in the write that deletes the chat
138e5b281	Merge pull request #383 from paritytech/feature/chat-request-cancellation
797113470	Merge branch 'develop' into feature/release-improvements
7f3639bd1	Merge pull request #442 from paritytech/feature/release-improvements
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 47 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
