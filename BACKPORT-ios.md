# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `5e1880542031992178a4eb0ffe1d3fa76ddaaac1` |
| to | `22407b10588d4b9af2ab197c2b5de2ba2ac6b0ef` |
| commits | 17 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#394](https://github.com/paritytech/polkadot-ios-community/pull/394)
- [#395](https://github.com/paritytech/polkadot-ios-community/pull/395)
- [#397](https://github.com/paritytech/polkadot-ios-community/pull/397)

<details>
<summary>All 17 commits</summary>

```
22f2c97aa	rfc models added
965884321	adopt container
a5fb34afd	add built in personhood key manager
666a0b05c	list product logic add
ab508ee98	create proof logic adoption
3d52fe285	fix ring vrf error mapping
a8cf5b7e9	Rework the coin materials, and keep the lighting under debug dials
1fe41fa2f	Turn the studio by the one rotation between gravity readings
857a9d581	Stop clipping the picked coin's band to the field's own end
c1e4924dd	Bake the studio on the device and ship the lifted studio
5d4af0fd6	Take the gyro reading outright and never stall the frame on the GPU
7cdb045fc	Merge pull request #394 from paritytech/feature/rfc-24
b2f32fd7e	Bake the studio in whole threadgroups
c81e2994b	fix: explain how each permission is used in purpose strings
5ab3b75ca	fix: keep only user-visible purpose string changes
e22f07891	Merge pull request #397 from paritytech/issue-396
22407b105	Merge pull request #395 from paritytech/fix/coinage-coin-materials
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 17 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
