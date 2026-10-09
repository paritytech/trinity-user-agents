# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `c9f5dda838589dbd2dc111a20022ee7bdf4f2504` |
| to | `1c18cc25f8f6e9351e4f3a6b327cf9b33abd5af4` |
| commits | 15 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#269](https://github.com/paritytech/polkadot-ios-community/pull/269)
- [#273](https://github.com/paritytech/polkadot-ios-community/pull/273)
- [#274](https://github.com/paritytech/polkadot-ios-community/pull/274)
- [#276](https://github.com/paritytech/polkadot-ios-community/pull/276)

<details>
<summary>All 15 commits</summary>

```
da5c8f3d7	Own call camera capturer creation in the engine actor and always create the video track
182c9ea0e	Add camera toggle to chat calls with camera off by default in audio calls
9b0d96e6b	Exchange call camera state with the peer over the Android media-state channel
977bf4daa	Send call microphone state to the peer and show mic-off banners
1ef8b7de0	Keep the call media-state subscriber draining and ignore repeated camera taps
ac9689415	Inject the camera permission service into call permissions
192b9ae1d	Alert on denied camera and roll call video back when capture fails
6ba7986b7	Fixed warning
78e12f8af	Merge pull request #269 from paritytech/feature/toggle-video-feed
84af0edb2	chore: remove the DIM subsystem
532b0a6b3	Removed todo
67f55fd93	Merge pull request #273 from paritytech/chore/remove-dims
1a30ddb8f	Merge pull request #274 from paritytech/feature/funding-buttons
d9fa5b97c	Drop the orphaned processing background mode
1c18cc25f	Merge pull request #276 from paritytech/fix/background-tasks
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
