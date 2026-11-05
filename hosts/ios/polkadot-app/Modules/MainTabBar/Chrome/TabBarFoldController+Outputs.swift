import UIKit

/// How the Edge glow's opacity is set: from the hide progress, or faded out once a settle toward shown starts.
enum TabBarEdgeGlowOpacity {
    case progress(CGFloat)
    case fadeOut
}

/// How the bar came to tuck: the user's swipe, or the auto-hide.
enum DockletTuckKind: Equatable {
    case user
    case autoHide
}

extension TabBarFoldController {
    /// Where the fold controller reports what the chrome must reflect.
    struct Outputs {
        /// Receives the Edge glow's rect and how its opacity is set. The glow is drawn outside the bar's transform.
        let glow: (CGRect, TabBarEdgeGlowOpacity) -> Void
        /// Receives the tucked bar's tap target, or `.zero` when it is not tucked.
        let grabZone: (CGRect) -> Void
        /// A state other than `.shown` must dismiss any open panel.
        let closePanel: () -> Void
        /// Reports each committed state.
        let state: (TabBarVisibilityState) -> Void
        /// Receives the content fade's target each time a transition starts it. The chrome animates the content to it.
        let contentFade: (DockletContentFadeTarget) -> Void
        /// Receives the kind of tuck that has just landed tucked.
        let tuckLanded: (DockletTuckKind) -> Void
    }
}
