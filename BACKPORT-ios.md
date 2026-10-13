# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `1c18cc25f8f6e9351e4f3a6b327cf9b33abd5af4` |
| to | `2f07dc947efd44d1c8cc5a272f93f1411f1582c1` |
| commits | 23 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#272](https://github.com/paritytech/polkadot-ios-community/pull/272)
- [#280](https://github.com/paritytech/polkadot-ios-community/pull/280)
- [#281](https://github.com/paritytech/polkadot-ios-community/pull/281)
- [#282](https://github.com/paritytech/polkadot-ios-community/pull/282)

<details>
<summary>All 23 commits</summary>

```
434288460	Retry chat attachment uploads dropped by the HOP transport
2acf29584	Retry HOP pool-full and rate-limited upload errors
66bd10474	[wip] onchain helpers
edb5db69a	add slot allocator
c90198e2f	wire chat request service with notification slot allocation and signing
dd06d53f8	add symbol counter to chat request
820eba766	failed message state
a9dac9bdb	Merge pull request #272 from paritytech/fix/file-uploads
94f4266e4	notification allocation renewal
4943bfdf5	integrate Sentry into chat requests
ab97c2553	update docs
10706338e	fix decoding
d70345273	Merge branch 'feature/anon-chat-requests' into feature/issue-monitoring-for-chat-requests
07464fee1	bump database
36efced2c	Merge branch 'feature/anon-chat-requests' into feature/issue-monitoring-for-chat-requests
8b05ee438	refactoring
2e2ef56fb	make issue reporting generic
310948313	add sentry integrate for allowances
d85560e4a	fix tests
4f890fe5d	Merge branch 'feature/issue-monitoring-for-chat-requests' into chore/use-onchain-timestamp
68472f883	Merge pull request #280 from paritytech/feature/anon-chat-requests
4f3e1d2a2	Merge pull request #281 from paritytech/feature/issue-monitoring-for-chat-requests
2f07dc947	Merge pull request #282 from paritytech/chore/use-onchain-timestamp
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 23 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
