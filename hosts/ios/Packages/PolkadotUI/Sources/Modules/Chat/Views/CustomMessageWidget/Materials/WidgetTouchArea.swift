import SwiftUI
import UIKit

/// A `Touch` node of a card, as the core decided it.
public struct WidgetTouchNode: Equatable, Sendable {
    public enum Drag: Equatable, Sendable {
        /// A drag mostly sideways. A drag mostly up or down scrolls the chat.
        case horizontal
        /// Every drag that starts inside the node.
        case any
    }

    /// The node's place among the tree's `Touch` nodes, depth first.
    public let index: UInt32
    public let drag: Drag
    /// False for a node past a limit, which draws its children like a box.
    public let takesTouch: Bool
    /// The actions of its tap and drag triggers, each offered to VoiceOver.
    public let accessibilityActions: [String]

    public init(index: UInt32, drag: Drag, takesTouch: Bool, accessibilityActions: [String]) {
        self.index = index
        self.drag = drag
        self.takesTouch = takesTouch
        self.accessibilityActions = accessibilityActions
    }
}

public enum WidgetTouchPhase: Sendable {
    case down
    case move
    case up
    case cancel
}

/// What a `Touch`, a `TouchState` and a `Replay` argument read in one frame.
public struct WidgetTouchInputs: Equatable, Sendable {
    public var touch: SIMD4<Float>
    public var state: SIMD2<Float>
    public var replay: Float

    public init(touch: SIMD4<Float>, state: SIMD2<Float>, replay: Float = 0) {
        self.touch = touch
        self.state = state
        self.replay = replay
    }

    /// Before any touch, and for a layer outside a `Touch` node.
    public static let none = WidgetTouchInputs(touch: .zero, state: [0, -1])
}

/// The trail a finger left: one byte a cell, row after row from the top left.
public struct WidgetTrailImage: Sendable {
    public let width: Int
    public let height: Int
    public let bytes: Data
    /// Moves each time a cell is marked.
    public let revision: UInt64

    public init(width: Int, height: Int, bytes: Data, revision: UInt64) {
        self.width = width
        self.height = height
        self.bytes = bytes
        self.revision = revision
    }
}

/// The core's state of one `Touch` node: the trail, the fired triggers, the last touch and the replay clock.
public protocol WidgetTouchTracking: AnyObject {
    /// Takes one finger sample in points from the node's top left, and returns the actions it fires.
    func touch(_ phase: WidgetTouchPhase, at point: CGPoint, time: TimeInterval) -> [String]
    func inputs(now: TimeInterval) -> WidgetTouchInputs
    func trail() -> WidgetTrailImage?
}

/// Makes the state of each `Touch` node of one tree.
public struct WidgetTouchFactory {
    /// One per `Touch` node, depth first, equal across trees exactly when the node's props are.
    let keys: [AnyHashable]
    let make: @MainActor (UInt32, CGSize) -> any WidgetTouchTracking

    public init(keys: [AnyHashable], make: @escaping @MainActor (UInt32, CGSize) -> any WidgetTouchTracking) {
        self.keys = keys
        self.make = make
    }
}

/// One `Touch` node as a card draws it, and the views to redraw on each finger sample.
@MainActor
public final class WidgetTouchArea {
    public let tracking: any WidgetTouchTracking
    let size: CGSize
    private let redraws = NSHashTable<UIView>.weakObjects()

    init(tracking: any WidgetTouchTracking, size: CGSize) {
        self.tracking = tracking
        self.size = size
    }

    func redraw(on view: UIView) {
        redraws.add(view)
    }

    /// Hands one finger sample to the core and redraws what reads it.
    func sample(_ phase: WidgetTouchPhase, at point: CGPoint) -> [String] {
        let actions = tracking.touch(phase, at: point, time: CACurrentMediaTime())
        redraws.allObjects.forEach { $0.setNeedsDisplay() }
        return actions
    }
}

private struct WidgetTouchAreaKey: EnvironmentKey {
    static let defaultValue: WidgetTouchArea? = nil
}

extension EnvironmentValues {
    var widgetTouchArea: WidgetTouchArea? {
        get { self[WidgetTouchAreaKey.self] }
        set { self[WidgetTouchAreaKey.self] = newValue }
    }
}
