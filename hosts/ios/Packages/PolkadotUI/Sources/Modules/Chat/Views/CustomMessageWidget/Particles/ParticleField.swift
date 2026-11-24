import QuartzCore
import SwiftUI
import UIKit

/// One particle in flight, in the node's frame from its top-start corner.
struct Particle: Equatable {
    var look: Int
    var start: CGPoint
    var speed: Double
    var direction: Double
    /// Degrees a second.
    var spin: Double
    var born: TimeInterval
}

/// The particles an emitter has sent and when it sends the next ones: a pure model the view moves on each frame.
struct ParticleField {
    typealias Emitter = CustomMessageWidgetNode.Emitter

    private(set) var particles: [Particle] = []
    /// Particles owed by the time-based rate, carried between frames.
    private var timeCarry: Double = 0
    /// Points the finger moved that have not yet sent a particle, for the distance-based rate.
    private var distanceCarry: Double = 0
    private var lastFinger: CGPoint?
    /// Where the pan source's finger last was, kept after it lifts, for its bursts.
    private var lastSeen: CGPoint?
    private var lastTick: TimeInterval?

    /// Uniform random numbers in 0 through 1, swappable for tests.
    var random: () -> Double = { Double.random(in: 0 ... 1) }

    var isEmpty: Bool { particles.isEmpty }

    /// The frames start again after a pause: the time between owes no particles.
    mutating func resume() {
        lastTick = nil
    }

    /// Moves time on to `now`: drops the particles whose life is over, then, while `sending`, sends what the source
    /// owes since the last frame. `finger` is the pan source's finger in the node's frame while it is down; `size`
    /// the node's.
    mutating func tick(_ emitter: Emitter?, sending: Bool, at now: TimeInterval, finger: CGPoint?, size: CGSize) {
        let elapsed = lastTick.map { max(now - $0, 0) } ?? 0
        lastTick = now
        if let finger { lastSeen = finger }
        let life = emitter?.life ?? 0
        particles.removeAll { now - $0.born >= life }
        guard sending, let emitter, !emitter.looks.isEmpty else {
            timeCarry = 0
            distanceCarry = 0
            lastFinger = finger
            return
        }
        switch emitter.source {
        case .pan:
            guard let finger else {
                lastFinger = nil
                distanceCarry = 0
                timeCarry = 0
                return
            }
            let from = lastFinger ?? finger
            lastFinger = finger
            timeCarry += emitter.ratePerSecond * elapsed
            for _ in 0 ..< take(&timeCarry) {
                send(emitter, from: finger, at: now)
            }
            emitAlong(emitter, from: from, to: finger, at: now)
        case .point,
             .area:
            timeCarry += emitter.ratePerSecond * elapsed
            for _ in 0 ..< take(&timeCarry) {
                send(emitter, from: start(emitter, size: size), at: now)
            }
        }
    }

    /// `count` particles at once from the source; a pan source sends them from where the finger last was.
    mutating func burst(_ emitter: Emitter, count: Int, at now: TimeInterval, finger: CGPoint?, size: CGSize) {
        let place: CGPoint? =
            switch emitter.source {
            case .pan: finger ?? lastSeen
            case .point,
                 .area: nil
            }
        if case .pan = emitter.source, place == nil { return }
        for _ in 0 ..< max(count, 0) {
            send(emitter, from: place ?? start(emitter, size: size), at: now)
        }
    }

    /// A particle's look, place, turn, opacity and scale at `now`, or nil once its life is over.
    func drawn(_ particle: Particle, _ emitter: Emitter, at now: TimeInterval)
        -> (place: CustomMessageWidgetNode.ParticlePlace, turn: Double)? {
        let age = now - particle.born
        guard let place = emitter.place(
            start: particle.start,
            speed: particle.speed,
            direction: particle.direction,
            age: age
        )
        else { return nil }
        return (place, particle.spin * age)
    }

    private mutating func emitAlong(_ emitter: Emitter, from: CGPoint, to: CGPoint, at now: TimeInterval) {
        guard emitter.ratePer100Points > 0 else { return }
        let length = hypot(to.x - from.x, to.y - from.y)
        let gap = 100 / emitter.ratePer100Points
        var along = gap - distanceCarry
        while along <= length {
            let share = length > 0 ? along / length : 1
            send(
                emitter,
                from: CGPoint(x: from.x + share * (to.x - from.x), y: from.y + share * (to.y - from.y)),
                at: now
            )
            along += gap
        }
        distanceCarry = length - (along - gap)
    }

    private func start(_ emitter: Emitter, size: CGSize) -> CGPoint {
        switch emitter.source {
        case let .point(point): point
        case .area: CGPoint(x: random() * size.width, y: random() * size.height)
        case .pan: .zero
        }
    }

    private mutating func send(_ emitter: Emitter, from place: CGPoint, at now: TimeInterval) {
        guard particles.count < emitter.maxAlive else { return }
        let pick = { (range: ClosedRange<Double>, share: Double) in
            range.lowerBound + (range.upperBound - range.lowerBound) * share
        }
        particles.append(Particle(
            look: min(Int(random() * Double(emitter.looks.count)), emitter.looks.count - 1),
            start: place,
            speed: pick(emitter.speed, random()),
            direction: emitter.angle + (random() - 0.5) * emitter.spread,
            spin: pick(emitter.spin, random()),
            born: now
        ))
    }

    private func take(_ carry: inout Double) -> Int {
        let whole = Int(carry.rounded(.down))
        carry -= Double(whole)
        return max(whole, 0)
    }
}
