import QuartzCore
import SwiftUI

/// How a node is drawn on top of the tree's own drawing, after layout and about its centre: opacity and scale
/// multiply, offsets and turn add.
struct MotionPose: Equatable {
    var opacity: Double = 1
    var scale: Double = 1
    /// Points, towards the end and downwards.
    var offset: CGSize = .zero
    /// Degrees, clockwise.
    var rotation: Double = 0

    static let identity = MotionPose()

    mutating func apply(_ property: CustomMessageWidgetNode.AnimatedProperty, _ value: Double) {
        switch property {
        case .opacity: opacity *= min(max(value, 0), 1)
        case .scale: scale *= value
        case .offsetX: offset.width += value
        case .offsetY: offset.height += value
        case .rotation: rotation += value
        }
    }
}

/// Where a `Follow` track's value is going and how it set off: drawn by the host on its own clock.
struct FollowRun: Equatable {
    var target: Double
    var from: Double
    var speed: Double
    var start: TimeInterval
    var motion: CustomMessageWidgetNode.Motion?

    /// A value the node first appears at, still.
    static func resting(at target: Double) -> FollowRun {
        FollowRun(target: target, from: target, speed: 0, start: 0, motion: nil)
    }

    func value(at now: TimeInterval) -> Double {
        let time = max(now - start, 0)
        switch motion {
        case nil:
            return target
        case let .timed(duration, curve):
            guard time < duration else { return target }
            return from + (target - from) * curve.ease(time / duration)
        case let .spring(omega, damping):
            let spring = SpringMotion(omega: omega, damping: damping)
            if spring.isDone(start: from - target, speed: speed, change: target - from, time: time) { return target }
            return target + spring.displacement(start: from - target, speed: speed, time: time)
        }
    }

    func speed(at now: TimeInterval) -> Double {
        let time = max(now - start, 0)
        switch motion {
        case nil:
            return 0
        case let .timed(duration, curve):
            guard time < duration else { return 0 }
            let step = 0.001, later = min(time + step, duration)
            return (target - from) * (curve.ease(later / duration) - curve.ease(time / duration)) / (later - time)
        case let .spring(omega, damping):
            return SpringMotion(omega: omega, damping: damping).speed(start: from - target, speed: speed, time: time)
        }
    }

    func isDone(at now: TimeInterval) -> Bool {
        let time = now - start
        switch motion {
        case nil:
            return true
        case let .timed(duration, _):
            return time >= duration
        case let .spring(omega, damping):
            return SpringMotion(omega: omega, damping: damping)
                .isDone(start: from - target, speed: speed, change: target - from, time: time)
        }
    }

    /// The tree changed the target at `now`: set off from the value drawn, keeping its speed on a spring.
    func retargeted(_ target: Double, motion: CustomMessageWidgetNode.Motion, at now: TimeInterval) -> FollowRun {
        guard target != self.target else { return self }
        let drawn = value(at: now)
        let moving = isDone(at: now) ? 0 : speed(at: now)
        return FollowRun(target: target, from: drawn, speed: moving, start: now, motion: motion)
    }
}

/// The enter motion's share of the way from its pose to how the tree draws the node, `elapsed` after it appeared.
enum EnterProgress {
    static func share(_ enter: CustomMessageWidgetNode.Enter, elapsed: TimeInterval) -> Double {
        let time = elapsed - enter.delay
        guard time > 0 else { return 0 }
        switch enter.motion {
        case let .timed(duration, curve):
            return time < duration ? curve.ease(time / duration) : 1
        case let .spring(omega, damping):
            return 1 + SpringMotion(omega: omega, damping: damping).displacement(start: -1, speed: 0, time: time)
        }
    }

    static func isDone(_ enter: CustomMessageWidgetNode.Enter, elapsed: TimeInterval) -> Bool {
        let time = elapsed - enter.delay
        guard time >= 0 else { return false }
        switch enter.motion {
        case let .timed(duration, _):
            return time >= duration
        case let .spring(omega, damping):
            return SpringMotion(omega: omega, damping: damping).isDone(start: -1, speed: 0, change: 1, time: time)
        }
    }

    static func pose(_ enter: CustomMessageWidgetNode.Enter, share: Double) -> MotionPose {
        let from = enter.from
        let mix = { (start: Double, end: Double) in start + (end - start) * share }
        return MotionPose(
            opacity: min(max(mix(from.opacity, 1), 0), 1),
            scale: mix(from.scale, 1),
            offset: CGSize(width: mix(from.offset.width, 0), height: mix(from.offset.height, 0)),
            rotation: mix(from.rotation, 0)
        )
    }
}

/// A keyframes track's timeline and when its clock started.
struct TimelineRun: Equatable {
    var keyframes: CustomMessageWidgetNode.Keyframes
    var start: TimeInterval
}

/// Everything a node's enter and tracks need to draw it at a moment.
struct NodeMotionState: Equatable {
    var appeared: TimeInterval?
    var timelines: [CustomMessageWidgetNode.AnimatedProperty: TimelineRun] = [:]
    var follows: [CustomMessageWidgetNode.AnimatedProperty: FollowRun] = [:]

    /// The node appeared at `now` with these tracks: timelines start, followed values stand at their targets.
    mutating func appear(tracks: [CustomMessageWidgetNode.Track], at now: TimeInterval) {
        appeared = now
        timelines = [:]
        follows = [:]
        update(tracks, at: now)
    }

    /// A tree arrived with these tracks: a changed timeline starts over and a changed target is moved to; a dropped
    /// track lets its property go back to how the tree draws it.
    mutating func update(_ tracks: [CustomMessageWidgetNode.Track], at now: TimeInterval) {
        var kept = Set<CustomMessageWidgetNode.AnimatedProperty>()
        for track in tracks {
            kept.insert(track.property)
            switch track.play {
            case let .keyframes(keyframes):
                follows[track.property] = nil
                if timelines[track.property]?.keyframes != keyframes {
                    timelines[track.property] = TimelineRun(keyframes: keyframes, start: now)
                }
            case let .follow(target, motion):
                timelines[track.property] = nil
                follows[track.property] = follows[track.property]?.retargeted(target, motion: motion, at: now)
                    ?? .resting(at: target)
            }
        }
        timelines = timelines.filter { kept.contains($0.key) }
        follows = follows.filter { kept.contains($0.key) }
    }

    func pose(enter: CustomMessageWidgetNode.Enter?, at now: TimeInterval, reduceMotion: Bool) -> MotionPose {
        var pose = MotionPose.identity
        if let enter, !reduceMotion {
            // Before it appears the node stands at the pose; once the motion ends it draws as the tree does.
            let elapsed = appeared.map { now - $0 }
            if !(elapsed.map { EnterProgress.isDone(enter, elapsed: $0) } ?? false) {
                pose = EnterProgress.pose(enter, share: elapsed.map { EnterProgress.share(enter, elapsed: $0) } ?? 0)
            }
        }
        for (property, run) in timelines {
            let value = reduceMotion ? run.keyframes.restingValue : run.keyframes.value(at: now - run.start)
            if let value { pose.apply(property, value) }
        }
        for (property, run) in follows {
            pose.apply(property, reduceMotion ? run.target : run.value(at: now))
        }
        return pose
    }

    func isSettled(enter: CustomMessageWidgetNode.Enter?, at now: TimeInterval) -> Bool {
        guard let appeared else { return false }
        if let enter, !EnterProgress.isDone(enter, elapsed: now - appeared) { return false }
        return timelines.values.allSatisfy { $0.keyframes.isOver(at: now - $0.start) }
            && follows.values.allSatisfy { $0.isDone(at: now) }
    }
}

/// Plays a node's `Enter` when it appears and its tracks on the host's clock, drawn after layout about its centre, as
/// `Translate` moves it. Where the person asked for less motion, the node draws as the tree does, timelines hold
/// their resting value and followed values change at once.
struct NodeMotionModifier: ViewModifier {
    let enter: CustomMessageWidgetNode.Enter?
    let tracks: [CustomMessageWidgetNode.Track]

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.layoutDirection) private var layoutDirection
    @State private var state = NodeMotionState()
    @State private var settled = false

    func body(content: Content) -> some View {
        if enter == nil, tracks.isEmpty {
            content
        } else {
            TimelineView(.animation(paused: settled || reduceMotion)) { timeline in
                let pose = state.pose(enter: enter, at: CACurrentMediaTime(), reduceMotion: reduceMotion)
                content
                    .scaleEffect(pose.scale)
                    .rotationEffect(.degrees(pose.rotation))
                    .offset(
                        x: layoutDirection == .rightToLeft ? -pose.offset.width : pose.offset.width,
                        y: pose.offset.height
                    )
                    .opacity(pose.opacity)
                    .onChange(of: timeline.date) {
                        if state.isSettled(enter: enter, at: CACurrentMediaTime()) { settled = true }
                    }
            }
            .onAppear {
                state.appear(tracks: tracks, at: CACurrentMediaTime())
                settled = false
            }
            .onChange(of: tracks) { _, tracks in
                state.update(tracks, at: CACurrentMediaTime())
                settled = false
            }
        }
    }
}
