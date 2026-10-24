# Backport hosts/android

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-android-community` into `hosts/android`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-android-community (`main`) |
| from | `148fcdb96184b32182c10ae12e398bca3d22b020` |
| to | `4b71fc3c6ef64c32bbd5d4e40f2f2a96fda748e2` |
| commits | 25 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#237](https://github.com/paritytech/polkadot-android-community/pull/237)
- [#265](https://github.com/paritytech/polkadot-android-community/pull/265)
- [#267](https://github.com/paritytech/polkadot-android-community/pull/267)

<details>
<summary>All 25 commits</summary>

```
678b6a352	Sign chat requests with unlinkable notification-slot accounts
551d76e46	Address review: reserve slots at scheduling, tighten request APIs
d14c17d8c	Address review: allocate as extension, mapToSet
6e9c6afe2	Logs
66fa7afcf	Address review: propagate cancellation from slot origin builders
5d97935fc	Sign chat messages with per-chat anonymous slot accounts
61b1fe4c9	Address review: plain chat signer, single time context, no signing before acceptance
c7be07693	Unify code with iOS spec
9bdcea51d	Merge origin/main into feature/unlinkable_chat_messages
272e03f0e	Align code with iOS commit from 19/10
1d70bfa87	Stop the SPA sheet flashing white before the product paints
5fb96c44c	Paint the SPA sheet in the main background colour
73493ee8a	Seed the app theme into the SPA sheet before the page loads
cdfdc5a58	Drop the SPA sheet theme seed
46572ee3b	align with ios
f20ec5985	Merge remote-tracking branch 'origin/main' into fix/spa-sheet-white-flash
0f003378f	Show the SPA sheet product only once it has painted the app theme
34e123f9a	Block touches and focus on the SPA sheet product until it is revealed
1fac5440c	Chunked upload
c2471d077	Comment fix
5eafdfc6a	Comment fix
ffa6e08a4	Merge pull request #237 from paritytech/feature/unlinkable_chat_messages
990914cdf	Merge pull request #265 from paritytech/fix/spa-sheet-white-flash
17c9bcdfb	Comment fix
4b71fc3c6	Merge pull request #267 from paritytech/feature/device-sync-update-chunks
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 25 commits into hosts/android`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-android.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
