# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `6737a076546ce152ab11d4f78a0799689745fc28` |
| to | `01fa06844256d4595089b70e06718db73463c682` |
| commits | 8 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#254](https://github.com/paritytech/polkadot-ios-community/pull/254)
- [#257](https://github.com/paritytech/polkadot-ios-community/pull/257)
- [#259](https://github.com/paritytech/polkadot-ios-community/pull/259)
- [#260](https://github.com/paritytech/polkadot-ios-community/pull/260)

<details>
<summary>All 8 commits</summary>

```
7136d4b37	don't check deadline if coins look failed
d2a6cbaae	fix error display
1000923e1	refactoring
9703d61bb	Merge pull request #254 from paritytech/fix/allow-multiple-request-accept
c9eee03c4	Merge pull request #257 from paritytech/fix/ramp-error
5141f954f	Merge pull request #259 from paritytech/fix/app-reset-remote-config
4bc8801be	Give Safetynet its own environment flag and remote signal
01fa06844	Merge pull request #260 from paritytech/chore/safetynet-env
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 8 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
