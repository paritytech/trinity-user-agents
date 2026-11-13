# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `6497046c7b7c7d2ff27774b417d99a83926110a5` |
| to | `bdd4416f262cd17b013384f91f1963d628f0b9c6` |
| commits | 13 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#406](https://github.com/paritytech/polkadot-ios-community/pull/406)
- [#417](https://github.com/paritytech/polkadot-ios-community/pull/417)
- [#420](https://github.com/paritytech/polkadot-ios-community/pull/420)

<details>
<summary>All 13 commits</summary>

```
1934a38ff	fix background execution
375178ea7	fix fallback in background
a47284743	fix comments
3474a02a4	Merge branch 'develop' into fix/background-task-conflict
3bc6d002b	add more logs
0237bd1dc	Merge branch 'develop' into fix/background-task-conflict
c0e2ebdd1	docs: document Firebase Remote Config as a known centralised dependency
276b8eb32	Merge pull request #417 from paritytech/docs/centralised-dependencies
93f25c89b	make claim retry window indefinite
961f6a3cb	fix docs
20d432922	remove MainActor artefact
a5020b1c2	Merge pull request #420 from paritytech/chore/remove-claiming-timeout
bdd4416f2	Merge pull request #406 from paritytech/fix/background-task-conflict
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 13 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
