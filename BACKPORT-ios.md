# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `b76a086bf41385819baa8cfd9a701b96929978b3` |
| to | `a15fbf250961324108a73b8ad819646181fe1421` |
| commits | 9 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#360](https://github.com/paritytech/polkadot-ios-community/pull/360)
- [#363](https://github.com/paritytech/polkadot-ios-community/pull/363)
- [#365](https://github.com/paritytech/polkadot-ios-community/pull/365)

<details>
<summary>All 9 commits</summary>

```
e87b9dbb9	Use Liquid Glass for tab bar materials on iOS 26
89cb542dd	Keep the centre panel's top edge still while the keyboard rises
aba69cd1a	Restore tab bar outline over Liquid Glass
8cce736f2	Restore lighter panel scrim blur under Liquid Glass
d497c7cd5	Tint tab bar glass with the theme's main surface on iOS 26
fa8234efb	fix: title the purse overlay instead of the asset symbol
934feb891	Merge pull request #360 from paritytech/fix/swipe-search-keyboard-jump
e3089658e	Merge pull request #363 from paritytech/fix/liquid-glass-tab-bar
a15fbf250	Merge pull request #365 from paritytech/issue-364
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 9 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
