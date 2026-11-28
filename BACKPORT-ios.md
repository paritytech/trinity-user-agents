# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `0af500310f5ff90d6e4c42f5a18b717f5ae8c754` |
| to | `d9e7a48a87388b2278d5b1a2334ae4037833ee36` |
| commits | 8 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#492](https://github.com/paritytech/polkadot-ios-community/pull/492)
- [#493](https://github.com/paritytech/polkadot-ios-community/pull/493)

<details>
<summary>All 8 commits</summary>

```
9dfa47114	Focus contact search from contacts list add button
7af26dc4f	Ask for device authentication only for transfers above the confirmation limit
ce94de5f8	Stop asking for device authentication before signing
4be33fe32	Stop asking for device authentication before a reclaim
78bbccde0	Ask for device authentication only for product payments above the confirmation limit
0ed21eab9	Gate product payments with an approval decorator and compare the confirmation limit in planks
e38ae03c2	Merge pull request #492 from paritytech/fix/add-contact-button
d9e7a48a8	Merge pull request #493 from paritytech/feature/risk-based-confirmation-gates
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
