import SwiftUI

/// The finger of a `Gesture` whose pan reports where it is, handed to the `Touch` nodes inside it.
///
/// The gesture's surface lies over its children and takes the finger before anything under it, so a `Touch` node
/// inside never sees it. It follows this feed instead: the same samples, at the same moment, on screen, which the
/// `Touch` node moves into its own frame.
@MainActor
final class GestureFingerFeed {
    /// The gesture's top-left corner on screen; the samples it sends are in its own frame.
    var origin: CGPoint = .zero
    private var followers: [ObjectIdentifier: (WidgetTouchPhase, CGPoint) -> Void] = [:]

    func follow(_ follower: AnyObject, _ receive: @escaping (WidgetTouchPhase, CGPoint) -> Void) {
        followers[ObjectIdentifier(follower)] = receive
    }

    func unfollow(_ follower: AnyObject) {
        followers[ObjectIdentifier(follower)] = nil
    }

    /// Sends a sample at `point` in the gesture's frame, as a place on screen.
    func send(_ phase: WidgetTouchPhase, at point: CGPoint) {
        let onScreen = CGPoint(x: point.x + origin.x, y: point.y + origin.y)
        followers.values.forEach { $0(phase, onScreen) }
    }
}

private struct GestureFingerKey: EnvironmentKey {
    static let defaultValue: GestureFingerFeed? = nil
}

extension EnvironmentValues {
    /// The finger feed of the nearest `Gesture` around that reports where the finger is.
    var gestureFinger: GestureFingerFeed? {
        get { self[GestureFingerKey.self] }
        set { self[GestureFingerKey.self] = newValue }
    }
}
