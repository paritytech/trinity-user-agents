# Backport hosts/ios

Work order. Everything below describes what is missing from this repository; nothing here has been applied yet.

Changes move one way, from `https://github.com/paritytech/polkadot-ios-community` into `hosts/ios`. Nothing in this repository is sent back the other way.

| | |
| --- | --- |
| source | https://github.com/paritytech/polkadot-ios-community (`develop`) |
| from | `26e4fa1c55d2d048e69fe9b469adbb78caf3b5fe` |
| to | `06169a99d3d0e0c67a912087280fcf6621706636` |
| commits | 86 |

## Pull requests in the range

Taken from the commit subjects, so a commit that landed without a number is not listed here. The full list is below it.

- [#334](https://github.com/paritytech/polkadot-ios-community/pull/334)
- [#337](https://github.com/paritytech/polkadot-ios-community/pull/337)

<details>
<summary>All 86 commits</summary>

```
348980bc9	chore(ios): remove unused colors from app asset catalog
e21948901	chore(ios): remove unused colors from PolkadotUI asset catalog
540b0b573	chore(ios): rename assetUSDC color to Latin characters
3843034e4	chore(ios): use DesignSystem colors for white, error and success in app target
52fdd9cc0	chore(ios): use DesignSystem colors in PolkadotUI and remove replaced colorsets
b33f3213d	chore(ios): remove unused icons from app asset catalog
b6eb9f810	chore(ios): remove unused icons from PolkadotUI asset catalog
6c972a297	feat(ios): ship products in every build, open funding ones modally
79e13260d	docs(ios): add a glossary and an ADR for the tab bar shell
59b16df6a	refactor(ios): move the floating widget stack from the tab bar chrome to its host
d3547cd8a	refactor(ios): remove the settings attention badge from the tab bar
340a52e10	feat(ios): show chain connection status in a sheet instead of a tab bar panel
b96abf577	feat(ios): add the notched SwiftUI tab bar container to PolkadotUI
87fcc26b6	fix(ios): draw the tab bar rim on the theme's foreground axis
297481951	feat(ios): add the morphing centre control to the SwiftUI tab bar
2b9c166bd	feat(ios): grow the SwiftUI tab bar into a shelf above the bar row
28eb131c7	feat(ios): run the tab bar chrome on the SwiftUI bar
a4b8e14c1	feat(ios): open the scanner in the SwiftUI center panel
08387ed49	feat(ios): grow the tab bar into a shelf of open products
eb22a6122	feat(ios): close a product from the shelf with a long press
39b4c9440	refactor(ios): delete the old tab bar
1ca5bceb4	feat(ios): restore contact search in the center panel
b55dccf96	fix(ios): keep the bar row still and grow the center panel upward
9d03ff74d	fix(ios): let SwiftUI own the tab bar's expansion animations
fd85c995a	feat(ios): close a shelf product from a context menu
c2161386a	feat(ios): fill the center panel while the scan search is focused
c3c24ae50	fix(ios): report denied camera access on the scan panel's label
88400cf28	fix(ios): stop the tab bar graying on light themes while an expansion is open
ee7f38539	fix(ios): animate the center panel rim with the clip on first open
07d08ecce	perf(ios): build the scan panel once per open and stop re-laying it out mid-morph
ce531e97e	perf(ios): start the scan panel's camera once the centre panel settles open
d35e605df	fix(ios): crop the embedded camera preview instead of scaling it
d850e006a	fix(ios): leave the scan panel's camera to its own teardown
379470990	fix(ios): let the centre panel's content sit under the grabber
76f5f8e7c	feat(ios): stop capture while the scan panel's search is focused
f5782bba1	fix(ios): drag the centre panel from the grabber without toggling it
c5012f80c	fix(ios): lift the bar with the keyboard smoothly and leave half the bar row under it
9db0b3bc6	fix(ios): track centre panel drags in global space so the grabber does not fight the finger
f9a81252c	fix(ios): snap the centre panel and shelf faster and open the panel to the bar's full width
96b559dd1	fix(ios): pad the shelf's bottom clear of the centre cutout
19e542e47	refactor(ios): ship the in-panel contact search in every configuration
d005d6e75	fix(ios): show a selection pill behind the selected tab
027ae1b30	fix(ios): recolour the Apps glyph when the theme changes
cf7e23004	fix(ios): give the centre control the tab bar's background
4cc6ce895	fix(ios): round and rim the tab bar cutout, resize the bar and hide it while searching
fac0cc2db	fix(ios): use the native back button on the theme screen
86d36448e	fix(ios): let a filled centre panel reach the chain status strip
f4be875a5	fix(ios): size the centre panel from the bar width before it is laid out
c62be8c56	refactor(ios): derive the tab bar notch radius from the centre button
141c0954d	refactor(ios): simplify the tab bar chrome and name the keyboard overlap
3848931bd	docs(ios): describe the current tab bar in the architecture notes
cfc17d828	fix(ios): address the tab bar branch review and stop the scanner without racing the preview
f0b3e987d	docs(ios): align the tab bar notes, glossary and review checklists with the code
bd96284e1	chore(ios): stop tracking CONTEXT.md
14861b551	feat(ios): rubber-band the center panel past fully open
1883e7c33	feat(ios): drag the center panel from its content
cbed48de7	feat(ios): hand the center panel drag between the results list and the panel
7c899bf42	feat(ios): put the selection pill on the Apps item while a product is mounted
438dc13bb	feat(ios): carry the selection pill onto the mounted product cell while the shelf is open
f63fdafdf	feat(ios): frame the centre plus with scan corners that fade as the panel opens
a852331d6	feat(ios): let the center panel content take drags under the grabber
95246e9f3	feat(ios): dim the shelf's connection status and settings icons
acdad80a8	fix(ios): hide the shelf counter when no apps are open
e07616b13	feat(ios): keep only the scan frame on the centre button and shrink and lower it as the panel opens
de71b4777	feat(ios): show tab bar tips on the SwiftUI bar, anchoring the scan tip to the center button
ef9694cee	feat(ios): gate products and in-panel search behind FEATURE_PRODUCTS and FEATURE_INPUT
ecd084b69	feat(ios): size and place the tab bar, notch and selection pill to the R1 geometry
7741857a7	feat(ios): add press feedback, the open scan frame size and the Close search label to the centre button
5efa47c62	feat(ios): make the scan panel's Scan state a 204 pt panel with a 110 pt camera
d09a2e169	feat(ios): drop the keyboard as every scan panel close starts and keep the panel 12 pt below the top safe area
db21a0484	feat(ios): draw the tab bar notch, centre panel and scan frame from the R1 outlines
3269b3411	feat(ios): give the tab bar and centre control the R1 glass, rims and shadow, and dim the screen behind the open panel
220fa3ea0	feat(ios): move the centre control on the R1 motion curves and draw the rims along a true diagonal
77c7c8405	feat(ios): play the R1 haptics on centre panel open, swipe, close, Scan and Search changes and slot taps
2b3491f1a	fix(ios): stop the scanner before detaching its preview so the panel closes without a stall
6af65e7c9	feat(ios): style the scan panel search field and results list to R1
28436b3df	fix(ios): close the shelf when its last product closes
02f4bb716	feat(ios): open the centre panel straight into Search on a swipe up, with a haptic at each halfway crossing
a50856142	docs(ios): describe the tab bar architecture and composition instead of its visual spec
7def3b7e2	feat(ios): adopt theme tokens, glass outline and design system type in the tab bar
227c06450	Merge branch 'develop' into feature/swiftui-tab-bar
cbcbebe1d	fix(ios): pass the center control content as a labelled argument
b9eea0e74	Split the tab bar controller and its chrome controller to stay under the file length limit
c728fea18	Merge pull request #334 from paritytech/chore/assets-cleanup
d5a02b298	Restore the Settings no-backup attention dot in builds without products
06169a99d	Merge pull request #337 from paritytech/feature/swiftui-tab-bar
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

4. Decide whether this backport is a breaking change. Read the pull requests listed above against `.claude/skills/semver-pr-title/SKILL.md`. If any of them makes a tester lose data or a session, reinstall, or find a feature gone, or makes a product change its code, retitle this pull request with `!`, for example `chore(hosts)!: backport 86 commits into hosts/ios`, and name what breaks in its description. The nightly announcement lists `!` titles first.

5. Delete this file. It is the work order, not part of the tree:

   ```bash
   git rm BACKPORT-ios.md
   ```

6. Commit the refreshed tree, `hosts/imports.json` with its new ref, and the deletion together, then push. That push is what starts CI on this pull request.
