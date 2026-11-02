import CoreGraphics

/// A tip the Docklet shows once per device, beside the Edge glow.
enum DockletTip: Hashable, CaseIterable {
    /// The first time the user tucks the bar with a swipe.
    case tucked
    /// The first time the bar tucks itself. Showing it also counts `tucked` as seen.
    case autoHidden
}

/// How the bar came to tuck: the user's swipe, or the auto-hide.
enum DockletTuckKind: Equatable {
    case user
    case autoHide
}

/// The tip a tuck shows, and the tips that count as seen once it shows.
struct DockletTipDecision: Equatable {
    let tip: DockletTip
    let tipsSeen: Set<DockletTip>
}

/// Decides which Docklet tip a tuck shows. Pure: the caller reads and writes the seen flags.
enum DockletTipPolicy {
    /// Furthest a released drag may leave the tip from its rest and still restore the bar.
    static let flingDistance: CGFloat = 56

    /// The tip for `tuck`, or nil when that tip was seen or another tab bar tip is on screen. A skipped tip stays
    /// unseen, so a later tuck can show it.
    static func decision(
        for tuck: DockletTuckKind,
        seen: Set<DockletTip>,
        isAnotherTipShowing: Bool
    ) -> DockletTipDecision? {
        guard !isAnotherTipShowing else {
            return nil
        }
        switch tuck {
        case .user:
            guard !seen.contains(.tucked) else {
                return nil
            }
            return DockletTipDecision(tip: .tucked, tipsSeen: [.tucked])
        case .autoHide:
            guard !seen.contains(.autoHidden) else {
                return nil
            }
            return DockletTipDecision(tip: .autoHidden, tipsSeen: [.tucked, .autoHidden])
        }
    }

    /// Whether a drag released `translationX` from the tip's rest restores the bar. Anything within the fling distance
    /// snaps back.
    static func restores(afterDragging translationX: CGFloat) -> Bool {
        abs(translationX) > flingDistance
    }
}
