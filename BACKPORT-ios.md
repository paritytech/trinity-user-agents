# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `f56c1139647155b499ecca0b9749a048a5b4af3b` |
| to | `26e4fa1c55d2d048e69fe9b469adbb78caf3b5fe` |
| commits | 11 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#284](https://github.com/paritytech/polkadot-ios-community/pull/284)
- [#322](https://github.com/paritytech/polkadot-ios-community/pull/322)
- [#324](https://github.com/paritytech/polkadot-ios-community/pull/324)
- [#333](https://github.com/paritytech/polkadot-ios-community/pull/333)

<details>
<summary>All 11 commits</summary>

```
f892b964b	fix: flag hardcoded UI strings in a build phase
34e79761a	fix: give the unreadable-file warning a line number
de46a7679	fix: soften the SPA loader to fgTertiary with a 30% fade
b8468b40c	fix: run the hardcoded string check on commit and in CI, not on build
28d96f142	Merge pull request #324 from paritytech/issue-319
ca42ce7ae	Tame the coin lighting, pick coins out, and make the cash card a wallet (#284)
94f94d26a	Keep the wallet photo's own shadows dark
470611847	Keep the wallet photo's own shadows dark (#333)
d57f7a48e	fix: gate the hardcoded string check on added lines only
5a067566d	fix: allow verbatim text and an ignore marker in the string check
26e4fa1c5	Merge pull request #322 from paritytech/issue-321
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 11 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
