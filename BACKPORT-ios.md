# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `e02cb5121da81987cb32e7da0d7377eb37b61229` |
| to | `b866a71f94474644a2f9910bd3da90165f47de22` |
| commits | 16 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#335](https://github.com/paritytech/polkadot-ios-community/pull/335)
- [#342](https://github.com/paritytech/polkadot-ios-community/pull/342)
- [#348](https://github.com/paritytech/polkadot-ios-community/pull/348)

<details>
<summary>All 16 commits</summary>

```
ddebcc896	feat: gate coin and voucher allocation on the ios installation backup
c07abaaea	Draw the picked coin in front of the block headers
bc6acd6ff	Merge remote-tracking branch 'origin/develop' into issue-287
e7318de16	Merge branch 'develop' into issue-287
2dc88b82e	feat: update the default cash logo on the purse card to the new emblem
6deffcea8	decouple coins evaluation from recycling
17c896cb9	don't report pending state if already registered
ab0e8edcb	inject window scene
4cd045c4b	fix flaky test
f3d12b909	fix flaky tests
70148e06b	refactoring
9641d5e01	Merge pull request #335 from paritytech/issue-287
fb0fa6640	Merge branch 'develop' into fix/coinage-coin-look
18f929f93	remove unused properties
f47d79084	Merge pull request #348 from paritytech/issue-346
b866a71f9	Merge pull request #342 from paritytech/fix/coinage-coin-look
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 16 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
