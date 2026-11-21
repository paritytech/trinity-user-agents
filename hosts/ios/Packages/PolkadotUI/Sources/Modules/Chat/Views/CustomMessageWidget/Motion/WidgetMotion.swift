import Observation
import UIKit

/// Steps settles frame by frame until `step` answers false. `step` is handed the seconds since the last frame.
@MainActor
protocol WidgetMotionFrames: AnyObject {
    func start(_ step: @escaping (Double) -> Bool)
}

/// The values of one body's pans, as the device moves them.
///
/// The product declares where each pan rests and hears where it came to rest; everything in between runs here, so
/// a frame never waits for the product. A new tree reconciles with what is on screen by three rules: a declared value
/// is applied only when it differs from the previous tree's, a finger outranks any declared change, and a tree that
/// leaves a pan out drops it.
@Observable
@MainActor
final class WidgetMotion {
    /// Whether a finger holds a pan, from the start of its drag until the settle after it ends.
    private(set) var isUserMoving = false
    private var values: [String: Double] = [:]

    @ObservationIgnored var onEvent: WidgetPanHandler?
    @ObservationIgnored private var pans: [String: PanState] = [:]
    @ObservationIgnored private let frames: any WidgetMotionFrames
    @ObservationIgnored private let reduceMotion: @MainActor () -> Bool

    init(
        frames: (any WidgetMotionFrames)? = nil,
        reduceMotion: @escaping @MainActor () -> Bool = { UIAccessibility.isReduceMotionEnabled }
    ) {
        self.frames = frames ?? DisplayLinkMotionFrames()
        self.reduceMotion = reduceMotion
    }

    /// The pan `id`'s value and the period drives read it modulo, if the body has that pan.
    func reading(_ id: String) -> (value: Double, period: Int?)? {
        guard let value = values[id], let state = pans[id] else { return nil }

        return (value, state.pan.period)
    }

    /// Takes the pans of a new tree, depth first.
    func apply(pans newPans: [CustomMessageWidgetNode.Pan]) {
        let ids = Set(newPans.map(\.id))
        for gone in pans.keys where !ids.contains(gone) {
            pans[gone] = nil
            values[gone] = nil
        }
        for pan in newPans {
            guard var state = pans[pan.id] else {
                pans[pan.id] = PanState(pan: pan)
                values[pan.id] = Double(pan.value)
                continue
            }

            // A value the card already rests on, such as the product confirming a turn, moves and reports nothing.
            let declaredChange = pan.value != state.pan.value
            let alreadyThere = state.settle == nil && values[pan.id] == Double(pan.value)
            state.pan = pan
            pans[pan.id] = state
            if declaredChange, !state.isHeld, !alreadyThere {
                move(pan.id, to: pan.value, curve: pan.animation.map(WidgetPanSettle.Curve.init), owner: .product)
            }
        }
        refreshUserMoving()
    }

    /// A finger took the pan. It stops any settle under way where it is, and the drag counts its steps from where
    /// that settle was going, so a drag can reverse a turn but not add a second one.
    func beginDrag(_ id: String) {
        guard var state = pans[id], let value = values[id] else { return }

        state.drag = Drag(start: value, origin: state.settle?.target ?? WidgetPanPhysics.nearest(value))
        state.settle = nil
        state.settleOwner = nil
        pans[id] = state
        refreshUserMoving()
        report(.began, for: state.pan)
    }

    func drag(_ id: String, travel: Double, extent: Double) {
        guard let state = pans[id], let drag = state.drag else { return }

        values[id] = WidgetPanPhysics.dragged(
            from: drag.start,
            origin: drag.origin,
            travel: travel,
            extent: extent,
            pan: state.pan
        )
    }

    func release(_ id: String, pointsPerSecond: Double, extent: Double) {
        guard var state = pans[id], let drag = state.drag, let value = values[id] else { return }

        let velocity = WidgetPanPhysics.velocity(pointsPerSecond, extent: extent, pan: state.pan)
        let target = WidgetPanPhysics.restTarget(value: value, velocity: velocity, from: drag.origin, pan: state.pan)
        state.drag = nil
        pans[id] = state
        move(id, to: target, curve: .spring(WidgetPanPhysics.releaseSpring), velocity: velocity, owner: .user)
    }

    /// The press went to something else, such as the system: the value returns to where the drag began.
    func cancelDrag(_ id: String) {
        guard var state = pans[id], let drag = state.drag else { return }

        state.drag = nil
        pans[id] = state
        report(.cancelled, for: state.pan)
        move(id, to: drag.origin, curve: .spring(WidgetPanPhysics.releaseSpring), owner: .cancelled)
    }

    /// One step forward, as the accessibility action asks.
    func step(_ id: String) {
        guard let state = pans[id], !state.isHeld, let value = values[id] else { return }

        let from = state.settle?.target ?? WidgetPanPhysics.nearest(value)
        move(id, to: from + 1, curve: state.pan.animation.map(WidgetPanSettle.Curve.init), owner: .product)
    }
}

private extension WidgetMotion {
    enum Owner {
        case user
        case product
        /// Returns to the drag's start and reports nothing more, since the product heard `cancelled`.
        case cancelled
    }

    struct Drag {
        /// The value when the finger took the pan.
        let start: Double
        /// The step the drag counts its steps from.
        let origin: Int
    }

    struct PanState {
        var pan: CustomMessageWidgetNode.Pan
        var drag: Drag?
        var settle: WidgetPanSettle?
        var settleOwner: Owner?
        var elapsed = 0.0

        var isHeld: Bool {
            drag != nil || settleOwner == .user
        }
    }

    func move(
        _ id: String,
        to target: Int,
        curve: WidgetPanSettle.Curve?,
        velocity: Double = 0,
        owner: Owner
    ) {
        guard var state = pans[id], let value = values[id] else { return }

        guard let curve, !reduceMotion(), value != Double(target) else {
            values[id] = Double(target)
            state.settle = nil
            state.settleOwner = nil
            pans[id] = state
            finish(state.pan, at: target, owner: owner)
            return
        }

        state.settle = WidgetPanSettle(start: value, target: target, velocity: velocity, curve: curve)
        state.settleOwner = owner
        state.elapsed = 0
        pans[id] = state
        refreshUserMoving()
        frames.start { [weak self] seconds in self?.advance(by: seconds) ?? false }
    }

    /// Steps every settle under way, and answers whether any is left.
    func advance(by seconds: Double) -> Bool {
        for (id, var state) in pans {
            guard let settle = state.settle, let owner = state.settleOwner else { continue }

            state.elapsed += seconds
            values[id] = settle.value(at: state.elapsed)
            guard state.elapsed >= settle.duration else {
                pans[id] = state
                continue
            }

            state.settle = nil
            state.settleOwner = nil
            pans[id] = state
            finish(state.pan, at: settle.target, owner: owner)
        }
        return pans.values.contains { $0.settle != nil }
    }

    func finish(_ pan: CustomMessageWidgetNode.Pan, at target: Int, owner: Owner) {
        refreshUserMoving()
        guard owner != .cancelled else { return }

        report(.settled(target), for: pan)
    }

    func report(_ event: WidgetPanEvent, for pan: CustomMessageWidgetNode.Pan) {
        guard let actionId = pan.actionId else { return }

        onEvent?(actionId, event)
    }

    func refreshUserMoving() {
        let moving = pans.values.contains(where: \.isHeld)
        if moving != isUserMoving {
            isUserMoving = moving
        }
    }
}

/// Steps once per screen refresh, by the time that actually passed, so a dropped frame does not slow a settle.
@MainActor
private final class DisplayLinkMotionFrames: NSObject, WidgetMotionFrames {
    private var link: CADisplayLink?
    private var step: ((Double) -> Bool)?
    private var previousTimestamp: CFTimeInterval?

    func start(_ step: @escaping (Double) -> Bool) {
        self.step = step
        guard link == nil else { return }

        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc private func tick(_ link: CADisplayLink) {
        let elapsed = link.timestamp - (previousTimestamp ?? link.timestamp - link.duration)
        previousTimestamp = link.timestamp
        guard step?(elapsed) == true else {
            link.invalidate()
            self.link = nil
            step = nil
            previousTimestamp = nil
            return
        }
    }
}
