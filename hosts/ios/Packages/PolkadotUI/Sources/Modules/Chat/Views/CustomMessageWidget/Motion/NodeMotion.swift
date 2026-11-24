import Foundation
import SwiftUI

public extension CustomMessageWidgetNode {
    /// An easing curve as CSS `cubic-bezier(x1, y1, x2, y2)` draws it (the renderer's `Curve`).
    struct Curve: Equatable {
        public let x1: Double
        public let y1: Double
        public let x2: Double
        public let y2: Double

        public init(x1: Double, y1: Double, x2: Double, y2: Double) {
            self.x1 = min(max(x1, 0), 1)
            self.y1 = y1
            self.x2 = min(max(x2, 0), 1)
            self.y2 = y2
        }

        public static let linear = Curve(x1: 0, y1: 0, x2: 1, y2: 1)

        /// The progress at `time`, both shares of the whole, as the core finds it (`Curve::ease`): fifty halvings of
        /// the curve's time.
        public func ease(_ time: Double) -> Double {
            let time = min(max(time, 0), 1)
            func bezier(_ first: Double, _ second: Double, _ at: Double) -> Double {
                let rest = 1 - at
                return 3 * rest * rest * at * first + 3 * rest * at * at * second + at * at * at
            }
            var low = 0.0, high = 1.0
            for _ in 0 ..< 50 {
                let middle = (low + high) / 2
                if bezier(x1, x2, middle) < time { low = middle } else { high = middle }
            }
            return bezier(y1, y2, (low + high) / 2)
        }
    }

    /// How the host moves a value to another (the renderer's `Motion`).
    enum Motion: Equatable {
        case timed(duration: TimeInterval, curve: Curve)
        /// A damped spring from rest: angular frequency in radians a second, and damping ratio.
        case spring(omega: Double, damping: Double)
    }

    /// Where a node enters from, relative to how the tree draws it (the renderer's `Pose`).
    struct Pose: Equatable {
        public var opacity: Double
        public var scale: Double
        /// Points, towards the end and downwards.
        public var offset: CGSize
        /// Degrees, clockwise.
        public var rotation: Double

        public init(opacity: Double = 1, scale: Double = 1, offset: CGSize = .zero, rotation: Double = 0) {
            self.opacity = min(max(opacity, 0), 1)
            self.scale = scale
            self.offset = offset
            self.rotation = rotation
        }

        public static let identity = Pose()
    }

    /// Plays a node in from a pose when it appears (the renderer's `Enter`).
    struct Enter: Equatable {
        public let from: Pose
        public let motion: Motion
        public let delay: TimeInterval

        public init(from: Pose, motion: Motion, delay: TimeInterval = 0) {
            self.from = from
            self.motion = motion
            self.delay = delay
        }
    }
}

public extension CustomMessageWidgetNode {
    /// A part of how a node is drawn that a track moves (the renderer's `AnimatedProperty`).
    enum AnimatedProperty: Equatable, Hashable {
        /// A share of the node's own opacity, 0 through 1.
        case opacity
        case scale
        /// Points towards the end.
        case offsetX
        /// Points downwards.
        case offsetY
        /// Degrees, clockwise.
        case rotation
    }

    /// One value of a timeline (the renderer's `Keyframe`): reached `at` from the start of a run, along `curve`.
    struct Keyframe: Equatable {
        public let at: TimeInterval
        public let value: Double
        public let curve: Curve

        public init(at: TimeInterval, value: Double, curve: Curve = .linear) {
            self.at = at
            self.value = value
            self.curve = curve
        }
    }

    /// A timeline the host plays from when the node appears (the renderer's `Keyframes`). Times are kept in whole
    /// milliseconds, as the core reads them.
    struct Keyframes: Equatable {
        /// At most 32 are drawn.
        public static let drawn = 32

        public let frames: [Keyframe]
        /// Runs played; zero plays for ever.
        public let runs: Int
        public let alternate: Bool

        public init(frames: [Keyframe], runs: Int = 0, alternate: Bool = false) {
            self.frames = Array(frames.prefix(Self.drawn))
            self.runs = runs
            self.alternate = alternate
        }

        /// The value `elapsed` after the timeline started, as the core finds it (`Keyframes::value_at`); nil with no
        /// frames.
        public func value(at elapsed: TimeInterval) -> Double? {
            var latest: UInt64 = 0
            let timed = frames.map { frame -> (ms: UInt64, value: Double, curve: Curve) in
                latest = max(latest, Self.milliseconds(frame.at))
                return (latest, frame.value, frame.curve)
            }
            guard let first = timed.first, let last = timed.last else { return nil }
            let run = last.ms
            guard run > 0 else { return last.value }
            let elapsed = Self.milliseconds(elapsed)
            let runsDone = elapsed / run
            let (index, within): (UInt64, UInt64) = runs != 0 && runsDone >= UInt64(runs)
                ? (UInt64(runs) - 1, run)
                : (runsDone, elapsed % run)
            let at = alternate && index % 2 == 1 ? run - within : within
            if let exact = timed.last(where: { $0.ms == at }) { return exact.value }
            if at < first.ms { return first.value }
            guard let next = timed.firstIndex(where: { $0.ms > at }), next > 0 else { return last.value }
            let from = timed[next - 1], to = timed[next]
            let time = Double(at - from.ms) / Double(to.ms - from.ms)
            return from.value + (to.value - from.value) * to.curve.ease(time)
        }

        /// What a timeline holds where the person asked for less motion: its end, or its first frame when it plays for
        /// ever.
        public var restingValue: Double? {
            runs == 0 ? frames.first?.value : value(at: .greatestFiniteMagnitude / 4)
        }

        /// Whether the timeline has stopped moving `elapsed` after it started.
        public func isOver(at elapsed: TimeInterval) -> Bool {
            guard runs != 0, let last = frames.map({ Self.milliseconds($0.at) }).max() else { return runs != 0 }
            return Self.milliseconds(elapsed) >= last * UInt64(runs)
        }

        private static func milliseconds(_ seconds: TimeInterval) -> UInt64 {
            seconds <= 0 ? 0 : UInt64(min((seconds * 1_000).rounded(), Double(UInt64.max / 2)))
        }
    }

    /// How a track sets its property (the renderer's `TrackPlay`).
    enum TrackPlay: Equatable {
        case keyframes(Keyframes)
        /// The value `target`, which the node first appears at; a later change moves there with `motion`.
        case follow(target: Double, motion: Motion)
    }

    /// Moves one part of how a node is drawn on the host's clock (the renderer's `Track`).
    struct Track: Equatable {
        public let property: AnimatedProperty
        public let play: TrackPlay

        public init(property: AnimatedProperty, play: TrackPlay) {
            self.property = property
            self.play = play
        }
    }
}

/// A damped spring from a displacement and a speed towards zero, in closed form.
struct SpringMotion: Equatable {
    let omega: Double
    let damping: Double

    /// The displacement `time` after starting at `start` moving at `speed`.
    func displacement(start: Double, speed: Double, time: Double) -> Double {
        let omega = max(omega, 0.01), damping = max(damping, 0)
        if damping < 1 {
            let damped = omega * (1 - damping * damping).squareRoot()
            let decay = exp(-damping * omega * time)
            return decay *
                (start * cos(damped * time) + (speed + damping * omega * start) / damped * sin(damped * time))
        }
        if damping == 1 {
            return exp(-omega * time) * (start + (speed + omega * start) * time)
        }
        let root = (damping * damping - 1).squareRoot()
        let fast = -omega * (damping + root), slow = -omega * (damping - root)
        let second = (speed - slow * start) / (fast - slow)
        return (start - second) * exp(slow * time) + second * exp(fast * time)
    }

    /// The speed `time` after starting at `start` moving at `speed`.
    func speed(start: Double, speed: Double, time: Double) -> Double {
        let omega = max(omega, 0.01), damping = max(damping, 0)
        if damping < 1 {
            let damped = omega * (1 - damping * damping).squareRoot()
            let decay = exp(-damping * omega * time)
            let sine = (speed + damping * omega * start) / damped
            return decay * ((damped * sine - damping * omega * start) * cos(damped * time)
                - (damping * omega * sine + damped * start) * sin(damped * time))
        }
        if damping == 1 {
            let rate = speed + omega * start
            return exp(-omega * time) * (rate - omega * (start + rate * time))
        }
        let root = (damping * damping - 1).squareRoot()
        let fast = -omega * (damping + root), slow = -omega * (damping - root)
        let second = (speed - slow * start) / (fast - slow)
        return (start - second) * slow * exp(slow * time) + second * fast * exp(fast * time)
    }

    /// Done once within a thousandth of `change` of the target and moving less than that in 1/60 s.
    func isDone(start: Double, speed: Double, change: Double, time: Double) -> Bool {
        let near = abs(change) / 1_000
        let now = displacement(start: start, speed: speed, time: time)
        let next = displacement(start: start, speed: speed, time: time + 1.0 / 60)
        return abs(now) <= near && abs(next - now) < max(near, 1e-9)
    }
}
