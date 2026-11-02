# Docklet motion runs on a display link, not a UIKit animator

The Docklet's hide progress drives the Drawbridge pose, the Edge glow and the content fade every frame, and a touch must catch the Docklet at the exact progress and velocity it has mid-flight. We drive hide progress with a `CADisplayLink` and the closed-form spring in `DockletSpring`, so progress is the single state every output reads.

## Considered Options

- `UIViewPropertyAnimator` with `UISpringTimingParameters`: rejected. Its initial velocity is normalised by the remaining distance, which breaks down when the Docklet is released next to its target, and reading the exact progress and velocity of a running spring to catch it mid-flight is awkward.
- `CASpringAnimation` on the layer: rejected for the same reason, and because the glow and the lift are not properties of one layer.

## Consequences

- The rest of the app uses UIKit animators. Do not convert the Docklet back without moving catch, glow, content fade and lift onto another single source of progress.
- The spring is critically damped with a 0.32 s period. A release faster than 4 progress per second is under-damped (ratio 0.78). A quiet auto-hide uses a 0.42 s period.
- Under Reduce Motion a settle fades over 0.22 s (`DockletReducedFade`) instead of running the spring.
- The shown-to-hidden slide uses a UIKit spring animator, because the display link drives only the shown and tucked poses.
- The spring and the landing rule (within 0.0015 after 80 ms, hard stop at 1.4 s) are pure functions and are tested without UIKit in `DockletMotionTests`.
