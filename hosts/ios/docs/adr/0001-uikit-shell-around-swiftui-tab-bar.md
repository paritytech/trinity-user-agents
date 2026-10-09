# 0001. UIKit shell around the SwiftUI tab bar

Status: Accepted

## Context

The bottom tab bar is being rebuilt in SwiftUI. SwiftUI draws the bar, the center panel's morph and the shelf. Three jobs still need UIKit:

1. **Fold inside an interactive transition.** The bar folds with the navigation transition, including an interactive back swipe. That needs a `UIViewControllerTransitionCoordinator`. SwiftUI animation cannot join the coordinator.
2. **Bottom clearance for UIKit content.** Content controllers take their bottom clearance from `additionalSafeAreaInsets.bottom`. The resting bar height has to reach them there.
3. **Passthrough hit testing.** The chrome is full-bleed and transparent. Only the bar, the folded grab area and the dismiss region while an expansion is open take touches. Everything else passes through to the content. SwiftUI publishes one boolean for "an expansion is open", and the container does the hit testing.

## Decision

Keep a thin UIKit shell, `TabBarBottomChromeController`, around the SwiftUI bar. The shell hosts the SwiftUI bar in a hosting controller inside a passthrough container view. It owns:

- the fold state machine and the visibility policy, driven from the transition coordinator;
- the hit testing of the container;
- publishing the resting bottom clearance as a read-only value.

The shell writes no insets itself. The host screen, `MainTabBarViewController`, writes `additionalSafeAreaInsets.bottom` on the tab and content controllers. It combines the published resting clearance with the height of the floating widget stack.

The shell does no other layout. The bar's geometry, the center panel, the shelf and their animations live in SwiftUI.

## Alternatives considered

**Host the SwiftUI bar directly, with no shell.** The bar would be the root of a `UIHostingController`, and the tab controller would own nothing else. This was rejected because each of the three jobs still needs UIKit. The fold would move on SwiftUI's timing, not the transition coordinator's. The bottom clearance would live in SwiftUI's safe area, which UIKit content controllers do not read. The transparent chrome would need a subclass of the hosting view to pass touches through. That subclass is the shell under another name, with the transition and inset bridging still missing.

## Consequences

- The shell is thin. It owns no layout beyond the three jobs above.
- Layout changes belong in the SwiftUI views. Changes to the fold or hit testing belong in the shell. Changes to the bottom inset belong in `MainTabBarViewController`.
- The shelf and the center panel overlay content and do not change the bottom inset. Clearance is the resting bar height only.
