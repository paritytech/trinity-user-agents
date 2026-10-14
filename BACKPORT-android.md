# Backport hosts/android

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-android-community` into `hosts/android`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-android-community (`main`) |
| from | `99cd9d29318b76a8ae5e6997cd9c58525bf780c3` |
| to | `99dddfe1b2331ca73997be5c875651bcaeec384b` |
| commits | 5 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#231](https://github.com/paritytech/polkadot-android-community/pull/231)
- [#233](https://github.com/paritytech/polkadot-android-community/pull/233)
- [#234](https://github.com/paritytech/polkadot-android-community/pull/234)
- [#242](https://github.com/paritytech/polkadot-android-community/pull/242)
- [#243](https://github.com/paritytech/polkadot-android-community/pull/243)

<details>
<summary>All 5 commits</summary>

```
e15e9684e	Rework the coinage coins after stakeholder feedback (#243)
1b05aafe0	Keep bottom nav tab labels on one line so the bar keeps its height (#242)
d6e779866	Show Get and Send on the CASH card and add a Send to yourself row (#233)
b39ddf62f	Sign chat requests with unlinkable accounts (#231)
99dddfe1b	Draw the CASH ticker with the CashLogo wordmark (#234)
```

</details>

## How to finish this

You are completing this pull request. Work on its branch and push to it.

1. Run the refresh. Do not merge the trees by hand:

   ```bash
   scripts/refresh-host-import.sh refresh android
   ```

   It replaces `hosts/android` with the source's tree, re-applies this repository's adaptations as a three-way patch, and compares every path against the source by blob hash in both directions. That comparison is the point: it is what catches upstream work silently dropped and adaptations that no longer apply. A hand-merge produces a plausible tree and none of those checks.

2. Read what it reports before committing.

   - A conflict is left unmerged on purpose, so git refuses to commit it. Resolve each one, keeping this repository's adaptation unless the source has clearly superseded it.
   - `adaptation left no difference` means either the source adopted that change or it did not apply. Check which, and say so in the pull request.
   - If the script refuses the result outright, do not force it. Recover with `git reset --hard HEAD` and say what happened.

3. Build what the change touches. An upstream change that adds a requirement to a protocol will not show up as a conflict, because the comparison reads content and not types; it shows up as a conformer in this repository that no longer compiles.

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 5 commits into hosts/android`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-android.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
