# Backport hosts/android

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-android-community` into `hosts/android`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-android-community (`main`) |
| from | `def76b2019703195e1e5df20a8243fbf663d29fd` |
| to | `eeb27c152f78a005d2589cc1a1785a73844f872e` |
| commits | 2 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#259](https://github.com/paritytech/polkadot-android-community/pull/259)

<details>
<summary>All 2 commits</summary>

```
e987f8672	Load a product link into its open tab when it names a page
eeb27c152	Merge pull request #259 from paritytech/fix/open-tab-loads-requested-page
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 2 commits into hosts/android`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-android.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
