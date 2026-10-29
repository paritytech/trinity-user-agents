import SwiftUI

/// The drag input a center panel's content feeds to the control hosting it. Translations and velocities are in
/// window space, positive downward, so the panel follows the finger rather than its own moving frame.
public struct DSTabBarCenterDrag {
    private let onChanged: (CGFloat) -> Void
    private let onEnded: (CGFloat) -> Void
    private let onCancelled: () -> Void

    init(
        onChanged: @escaping (CGFloat) -> Void,
        onEnded: @escaping (CGFloat) -> Void,
        onCancelled: @escaping () -> Void
    ) {
        self.onChanged = onChanged
        self.onEnded = onEnded
        self.onCancelled = onCancelled
    }

    /// Moves the panel with the finger.
    public func changed(translationHeight: CGFloat) {
        onChanged(translationHeight)
    }

    /// Settles the panel from where the release leaves it, by the velocity the finger leaves with.
    public func ended(velocityHeight: CGFloat) {
        onEnded(velocityHeight)
    }

    /// Settles the panel to the nearer end when the gesture is interrupted.
    public func cancelled() {
        onCancelled()
    }
}

public extension EnvironmentValues {
    /// Set on the content of an open center panel. Nil elsewhere.
    var centerPanelDrag: DSTabBarCenterDrag? {
        get { self[CenterPanelDragKey.self] }
        set { self[CenterPanelDragKey.self] = newValue }
    }
}

private struct CenterPanelDragKey: EnvironmentKey {
    static let defaultValue: DSTabBarCenterDrag? = nil
}
