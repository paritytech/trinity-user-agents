# Pocket Transitions

This doc describes how a screen pushed on a navigation stack draws its own push and pop, and lets a left-edge swipe scrub its pop. Pocket items use it today: an Opened card, the Balance card's screen (titled Purse) and Collectibles. Any further screen with a custom push and pop follows the same approach; [Adding a Screen](#adding-a-screen) lists what it implements. The doc also covers the pull-down close and the fade used under Reduce Motion or when the Pocket has no slot for the item. For how the Docklet follows an interactive pop, see [tab-bar-container.md](tab-bar-container.md).

## Summary

A Pocket item's screen is a pushed screen that conforms to `NavigationTransitionAnimating`. For each push and pop it returns its own animator from `navigationAnimator(for:)`, and `AppNavigationController` hands that animator to UIKit. A screen whose item flies conforms to `PocketFlightScreen` too and returns a `PocketFlight`. The card is not moved from the Pocket to the screen. The Pocket and the screen each draw a copy: the Pocket draws its copy during the flight, and the screen draws its copy at rest. A flight runs on a clock, or a finger drives it one frame at a time.

## The Approach

These rules hold for every screen that draws its own push and pop. The concrete screens below are examples of them.

1. **The screen owns its motion, the stack owns the input.** The screen conforms to `NavigationTransitionAnimating`. It returns a new animator for each push and pop, and says through `allowsBackSwipe` when a back swipe may start. `AppNavigationController` owns the edge pan, starts the pop and connects the animator to UIKit. A screen never adds its own back-swipe recognizer.
2. **One animator serves both a tap and a finger.** The pop animator conforms to `UIViewControllerAnimatedTransitioning` for the timed path and to `BackSwipeScrubbing` for the scrubbed path. Both paths share one setup step. `animateTransition` returns at once when the context is interactive, so a pop never runs both.
3. **Motion is a pure function of progress.** What is on screen at progress 0 to 1 comes from one pure model, such as `PocketFlightChoreography`. A finger applies a frame with no animation. A release animates from the current frame to frame 0 or frame 1. Progress 0 and 1 match the start and end of the timed pop, and tests prove it.
4. **The commit rule and release timing are shared.** The stack reads the finger with `BackSwipe.progress` and `BackSwipe.commits`, and passes the result to the animator. A release runs the remaining share of the timed duration on the same curve, with a floor so a short leg does not flash.
5. **Both sides of a shared element run one clock.** When something is drawn by two screens, as a card by the Pocket and by its screen, each side animates itself, on the same curve and duration. Neither side animates the other's views.
6. **Every exit restores the screen.** A completed push, a completed pop and a cancelled pop all end in one place, such as `endFlight()`, which puts back everything the transition changed. A cancelled pop leaves the screen on the stack, usable.
7. **The swipe is offered only when it can run.** `allowsBackSwipe` is false while the screen's own transition or another gesture, such as a pull-down, is running. The screen turns the system back swipe off while it is shown, so the stack's edge pan is the only one.
8. **A fade is the fallback.** Under Reduce Motion, or when the motion has nowhere to go, the screen fades. The fade is scrubbed the same way, through `PocketFadeScrub`.
9. **Frames are measured where they are drawn.** Any frame the animator measures, such as `itemFrame(in:)`, includes every offset already applied, counted once.

## Layers

```
AppNavigationController                  owns the left-edge pan and the scrubbed pop
│  asks the moving screen, through NavigationTransitionAnimating,
│  for its animator and allowsBackSwipe
▼
Screen: NavigationTransitionAnimating    owns its page, its card copy and any pull-down
│  + PocketFlightScreen                  when its item flies on the Pocket curve
│                                        e.g. ExpandedPocketCardViewController (Opened card),
│                                        AssetDetailsScreenViewController (Purse)
│  or its own animators                  e.g. CollectiblesViewController, see Collectibles
▼
PocketFlight                             one per transition; timed and scrubbed
│  PocketFlightChoreography              pure: progress to frame
│  PocketFadeScrub                       fade path under a finger
▼
PocketOrigin (WalletMainViewController)  owns the Pocket's slots, open item and clearance
▼
Pocket SwiftUI (WalletView)              draws each card from PocketCardMotion
```

## The Two Seams

`PocketFlightScreen` (`PocketFlight.swift`) is what a flight needs from a screen. It also inherits `pocketItem` and `statusStripTitle` from `PocketItemScreen`.

- `itemFrame(in:)`: the card's frame in a view, with any offset already applied.
- `beginFlight()`: runs before layout. It clears the screen's background so the Pocket shows through.
- `moveItem(by:)`: moves the screen's card and its page by an offset from rest.
- `setItemHidden(_:)`: hides the screen's card while the Pocket draws it.
- `setPageAlpha(_:)`: fades the page under the card.
- `endFlight()`: restores everything the flight changed.

`PocketOrigin` (`PocketOrigin.swift`) is what a flight needs from the Pocket. `WalletMainViewController` is the only conformer.

- `frame(of:)`: the item's slot in window coordinates. It returns nil when the Wallet tab is not selected, the Pocket is not directly under the item, or the item has no frame. A nil slot makes the flight a fade.
- `openItem` and `setOpenItem(_:)`: the item whose screen is up.
- `moveOpenItem(by:clearance:animatedOver:)`: the open item's offset and the Pocket's clearance. A duration animates the change on the flight's curve. Nil applies it at once. The extension `moveOpenItem(by:clearingPocket:animated:)` wraps it.

`pocketClearance` is 0 at rest and 1 when cleared. `PocketCardMotion.of` mixes each card by it. Cards below the open card slide off the bottom. Cards above it shrink away. The open card takes `openPocketCardOffset`.

**One clock.** `PocketFlightTiming` holds the duration (0.5 s) and the cubic curve. The screen uses it through `UICubicTimingParameters`. The Pocket uses it through `PocketFlightTiming.animation(duration:)` in a SwiftUI transaction. Both sides run the same curve for the same time. A push animates the Pocket from rest to cleared. A pop clears the Pocket at once, with no animation, and then animates it back to rest.

## Push

1. `navigationController(_:animationControllerFor:...)` asks the pushed screen, if it is a `NavigationTransitionAnimating`, for `navigationAnimator(for: .push)`. A `PocketFlightScreen` returns `PocketFlight(opening: true)`. A screen that is not a `NavigationTransitionAnimating` gets UIKit's default push.
2. `prepare` adds the screen's view at its final frame, calls `beginFlight()`, lays out, and measures the slot. The slot offset is the slot's frame minus `itemFrame(in:)`. It is nil when `frame(of:)` returns nil.
3. The motion from the table above runs.
4. **Fly.** The screen hides its card and moves its card and page to the slot, with the page at alpha 0. On the next main turn, so the start is drawn first, the Pocket clears to 1 and moves its item to the open place. The screen's card and page animate to rest. The page fades in from 35% into the flight.
5. **Fade or crossfade.** `fadeInPlace` fades the screen in over the Pocket. See Fade Path.
6. `finish` calls `endFlight()` and completes the transition.

The Pocket stays cleared while the item is open. `openItem` is set in `willMove(toParent:)`.

`PocketNavigation.show` (`PocketNavigation.swift`) decides how the screen reaches the Wallet stack. If the Pocket is on top and the push is animated, it pushes, which runs the flight. If another Pocket item is on top, it cross-dissolves (0.5 s) to the new stack. Otherwise it sets the stack without animation.

## Timed Pop

A pop that no finger drives runs the reverse. The pull-down close is one such pop, through `close()`.

1. `navigationAnimator(for: .pop)` returns `PocketFlight(opening: false)` for the screen being popped.
2. `prepare` places the Pocket under the screen at its final frame and measures the slot from where the card sits.
3. **Fly.** The Pocket clears at once to the card's open place. The card moves to the slot on the flight clock. Then the Pocket moves its item back to the slot and clears to 0. The page fades out over the first 20% of the flight (0.1 s).
4. **Fade.** `PocketFlight.fade` restores the Pocket at once and fades the screen out.
5. `finish` calls `endFlight()`. `didMove(toParent:)` then clears `openItem` and returns the Pocket to rest, unless another item has taken `openItem`.

## Pull-Down Close

`PocketPullDown` gives a `PocketFlightScreen` a pull to close. The screen installs it on its view and passes its own close. Examples are the Opened card and Purse.

- The pan starts only on the card, when the vertical velocity is larger than the horizontal one. It does not start during a transition. A screen can turn it off with `isEnabled`. The Opened card does so while the page covers the card (`setFaceCovered`).
- At begin, it snapshots the card and the body, and blurs a copy of the body. The overlay goes over the screen. The body is a snapshot because a Pocket card's body is a web view that cannot follow the finger.
- While the finger moves, the overlay's card and body move down by the travel. The body fades to `1 - progress`. The blurred copy rises to `progress`, up to 12 pt of blur. `PocketClosePull` computes the progress against `DSCardCollapse.distance(in:)`.
- A release past the collapse distance (`PocketClosePull.closes`) runs `closeCard`. It moves the real card to the overlay card's spot with `moveItem(by:)`, sets the page alpha to the body's alpha, removes the overlay, and calls `close()`. The pop is then a timed pop, which measures from that spot.
- A release short of the distance, or a cancelled pan, runs `springBack`. It returns the overlay to rest over 0.45 s and removes it.

**Invariant.** `itemFrame(in:)` returns the card where it is drawn, with the offset from `moveItem(by:)` counted exactly once. A flight measures from that frame. A screen that reads its card's frame from UIKit gets this by converting through the moved view, as the Opened card does through its stage. A screen that reads it from SwiftUI must store the frame without the offset and add the current offset when it reads it, as Purse does (`AssetDetailsScreenState.cardFrame` and `itemOffset`). A SwiftUI global frame may or may not include the hosting view's transform, depending on whether layout has run since the move.

## Scrubbed Pop (Left-Edge Swipe)

`AppNavigationController` owns the swipe. A screen takes part through `NavigationTransitionAnimating` alone: it answers `allowsBackSwipe` and returns an animator. The swipe scrubs the pop when that animator is a `BackSwipeScrubbing`, as `PocketFlight` and `CollectiblesBookReturn` are. The default `allowsBackSwipe` is false, so other screens keep the system back swipe.

- `backSwipe` is a left-edge `UIScreenEdgePanGestureRecognizer`. It begins only when the stack holds more than one controller, no transition runs, and `topViewController.allowsBackSwipe` is true. Other pan recognizers wait for it to fail, which it does at once for a touch away from the edge.
- `.began` calls `popViewController(animated: true)`. UIKit then asks `interactionControllerFor`. That method returns the animator as `scrubbedPop` only while the swipe is `began` or `changed`. Any other pop stays timed.
- `startInteractiveTransition` runs the same `prepare` as a timed pop. A fly hides the screen's card and scrubs `PocketFlightChoreography(toSlot:)`. A release that UIKit reported before this call replays here.
- `animateTransition` returns at once for an interactive transition.
- **Changed.** `BackSwipe.progress(travel:width:)` goes to `update(progress:)`. The frame from `choreography.frame(progress:)` applies with no animation: the card's offset, the page alpha, the Pocket's offset and the clearance. The context updates with the same progress.
- **Ended.** `BackSwipe.commits` decides. The travel must pass 30% of the width, or a fling must coast past half the width at more than 300 pt/s.
- `release` finishes or cancels the context. The rest runs to the end frame: frame 1 on commit, frame 0 on cancel. It runs on the flight's curve. Its duration is the flight's 0.5 s scaled by the share still to go, never under 0.15 s. The Pocket runs on the same duration.
- `finish` calls `endFlight()`. A cancelled pop leaves the screen on the stack with its card back in place.

Under Reduce Motion, or with no slot, the swipe scrubs a fade instead. See Fade Path.

## Fade Path

The fade runs when the motion is `fade` (no slot) or `crossfade` (Reduce Motion).

- **Timed.** `PocketFlight.fade` puts a `bgSurfaceMain` backdrop under the screen and fades the screen and the backdrop. The duration is 0.5 s on the flight curve, or 0.18 s under Reduce Motion (`fallbackAnimator`).
- **Scrubbed.** `PocketFadeScrub` restores the Pocket to rest, adds the backdrop, and fades with the finger. On release it fades the rest in 0.18 s times the share still to go. A commit ends at alpha 0. A cancel returns to alpha 1. The backdrop is removed when the fade ends.

`PocketFadeScrub` and `PocketFlight.fade` are shared with `CollectiblesBookFlight` and `CollectiblesBookReturn`.

## Collectibles

Collectibles is the example of a `NavigationTransitionAnimating` screen that is not a `PocketFlightScreen`. It returns its own pair of animators from `CollectiblesBookFlight.swift`: `CollectiblesBookFlight` for the push and `CollectiblesBookReturn`, a `BackSwipeScrubbing`, for the pop. It has no pull-down. It reuses `PocketFlight.motion`, `PocketFlight.fade`, `fallbackAnimator`, `duration(reduceMotion:)` and `PocketFadeScrub`.

The book is the Pocket's peek. Setting `.collectibles` as the open item lifts the peek. `WalletMainViewController` holds its bottom safe-area change while the peek is pressed or lifted and the Pocket is not cleared, so the book lifts from where it sits.

**Push.** With a slot, the book starts at the peek's place (`beginFlight(from:)`). The backdrop fades to `bgSurfaceMain` over 0.18 s. The book settles to rest on a critically damped spring (stiffness 260). The Pocket clears on the flight clock. Without a slot, or under Reduce Motion, the screen fades.

**Opening.** `viewIsAppearing` starts the book's opening: a lift that tilts the book (0.4 s), then a cover that swings open (0.22 s), which reveals the page. Under Reduce Motion, `openWithoutMotion` shows the page at once and skips the book. The page expands to fill the screen (0.25 s) after the product has loaded (`CollectiblesOpening`). A load that runs past `CollectiblesBookTiming.loadTimeout` (10 s) fails the page.

**Back swipe allowed.** `allowsBackSwipe` is true only after the opening has started, while no book animation runs, and with no close pending. A close requested during a book animation waits for that step to finish (`pendingClose`).

**Timed pop.** `close(then:)` winds the opening back. An expanded page shrinks to its resting inset. An open cover closes, and the page fades. The book lifts. Then `flyBack` moves the book to the peek on the same spring, clears the backdrop, and restores the Pocket on the flight clock.

**Scrub.** The finger scrubs the page, not the book. An expanded page shrinks by a paused animator driven through `fractionComplete`. A resting page fades its alpha. The book stays where it is while the finger moves. A committed release finishes the shrink, then runs the close steps. It skips the collapse the swipe already drew (`isPageShrunkBySwipe`), and flies the book back. A cancelled release restores the page to full size and completes as cancelled.

## Rules

- One pure function from progress to frame: `PocketFlightChoreography.frame(progress:)`. A finger sets it, and a release lands on its ends.
- One `PocketFlight` per transition. `navigationAnimator(for:)` creates it.
- The navigation stack owns the swipe and the scrubbed pop. A screen takes part only through `NavigationTransitionAnimating`: it answers `allowsBackSwipe` and returns an animator.
- Screens that allow the swipe turn the system back swipe off in `viewDidAppear` and on in `viewWillDisappear`. The edge pan on `AppNavigationController` is their only back swipe.
- `endFlight()` restores everything a flight changed, on every exit path, including a cancelled pop.
- `setOpenItem` runs in `willMove(toParent:)` with a parent. It clears in `didMove(toParent:)` without one.

## Tests

Pure models have unit tests. The wiring does not, and the simulator checks it.

- `polkadot-appTests/Products/PocketFlightTests.swift`: the motion choice.
- `polkadot-appTests/Products/PocketFlightChoreographyTests.swift`: frames and release duration.
- `polkadot-appTests/Products/PocketClosePullTests.swift`: pull progress and the close threshold.
- `polkadot-appTests/Products/PocketCardMotionTests.swift`: card motion under clearance.
- `polkadot-appTests/Common/Navigation/BackSwipeTests.swift`: swipe progress and the commit rule.
- `polkadot-appTests/Products/CollectiblesOpeningTests.swift`: the Collectibles opening.

`PocketPullDown`, `PocketFadeScrub`, the flight classes' UIKit calls and the gesture rules in `AppNavigationController` have no unit tests.

## Adding a Screen

Every screen with its own push and pop does these steps:

1. Conform to `NavigationTransitionAnimating`. Return a new animator from `navigationAnimator(for:)` for `.push` and `.pop`.
2. Keep an in-transition flag. Set it when an animator is requested and clear it when the transition ends, on every path. Return `allowsBackSwipe` as true only when the flag is clear and no other gesture of the screen is running.
3. Turn the system back swipe off in `viewDidAppear` and on in `viewWillDisappear`.
4. Make the pop animator a `BackSwipeScrubbing`. Share one setup step between `animateTransition` and `startInteractiveTransition`, and return from `animateTransition` when the context is interactive. Keep a release that arrives before `startInteractiveTransition`, and replay it there.
5. Put the motion in a pure model from progress to what is drawn, and test that its ends match the timed transition.
6. In `release(progress:commits:)`, take `commits` as the stack decided it with `BackSwipe.commits`. Finish or cancel the context, then animate the rest over the remaining share of the timed duration.
7. Restore the screen in one place on every exit, including a cancelled pop.
8. Fall back to a fade under Reduce Motion and when the motion has nowhere to go. Use `PocketFlight.fade` for the timed fade and `PocketFadeScrub` for the scrubbed one.

**A new Pocket item that flies on the Pocket curve** gets most of this from `PocketFlight`:

1. Add a case to `PocketItem` (`PocketOrigin.swift`).
2. Conform to `PocketFlightScreen` and return `PocketFlight(screen: self, origin: origin, opening:)` for push and pop. `PocketFlight` covers steps 4 to 8. The screen's `endFlight()` restores it and clears the in-transition flag.
3. Call `origin.setOpenItem` in `willMove(toParent:)` and clear it in `didMove(toParent:)`.
4. Place the item in the Pocket. `WalletMainViewController.frame(of:)` returns its slot, and `WalletView` (`isCardOpen`, `openPosition`) places it in the stack. A nil slot gives the fade path.
5. For a pull-down, install `PocketPullDown`, read `isPulling` in `allowsBackSwipe`, and keep `itemFrame(in:)` true to where the card is drawn.

**A screen with a motion of its own** writes its own animator pair, as Collectibles does with `CollectiblesBookFlight` and `CollectiblesBookReturn`. It still follows steps 1 to 8, and can reuse `PocketFlight.motion`, `PocketFlight.fade`, `PocketFadeScrub`, `BackSwipe` and `PocketFlightTiming`.
