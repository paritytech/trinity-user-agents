# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `2f07dc947efd44d1c8cc5a272f93f1411f1582c1` |
| to | `46ad9ccb48a20649ff350e8dad83fb3b55916e24` |
| commits | 15 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#291](https://github.com/paritytech/polkadot-ios-community/pull/291)
- [#292](https://github.com/paritytech/polkadot-ios-community/pull/292)
- [#298](https://github.com/paritytech/polkadot-ios-community/pull/298)

<details>
<summary>All 15 commits</summary>

```
2ce680792	implement rpc v2
87d989f8e	fix docs
494d6bed5	refactoring
a48c9c7d4	feat: render the cash with the cash lgo font across amount views
01e5e3115	Merge pull request #291 from paritytech/issue-289
50db8b20a	Show Connecting status while call is connecting
d32c29294	Observe remote media state after presenter is wired
4cd5bf2bf	Bypass CallKit on simulator so calls stay up
80391c580	Merge pull request #292 from paritytech/fix/video-call-issues
856b66865	refuse reconnection on app suspension
97a6105a6	make socket management TruAPI concern
b02fe278a	fix docs
7fbeb2035	fix review
62fbdf35f	refactoring
46ad9ccb4	Merge pull request #298 from paritytech/feature/rust-core-separate-connection
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 15 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
