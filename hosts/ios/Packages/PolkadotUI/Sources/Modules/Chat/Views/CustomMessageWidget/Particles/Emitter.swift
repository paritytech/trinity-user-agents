import SwiftUI

public extension CustomMessageWidgetNode {
    /// Sends particles from a node, drawn and moved by the host on its own clock (the renderer's `Emitter`). Places
    /// and sizes are in points, from the node's top-start corner; speeds a second.
    struct Emitter: Equatable {
        /// The most particles alive at once a host draws.
        public static let mostAlive = 512

        public enum Look: Equatable {
            case circle(diameter: CGFloat, color: Color)
            case rectangle(size: CGSize, color: Color)
            /// Fetched as an image node's picture is; nil, or one that cannot be fetched, draws no particle.
            case image(ImageSource?, size: CGSize)
        }

        public enum Source: Equatable {
            /// Where the finger is on the keyed location pan, while it is down.
            case pan(String)
            case point(CGPoint)
            /// Anywhere in the node, evenly at random.
            case area
        }

        public enum Trigger: Equatable {
            case appear
            /// The host sending this action from the body's current tree.
            case action(String)
            /// A tree carrying a different value than the tree before.
            case change(UInt32)
        }

        public struct Burst: Equatable {
            public let count: Int
            public let on: Trigger

            public init(count: Int, on: Trigger) {
                self.count = count
                self.on = on
            }
        }

        public var looks: [Look]
        public var source: Source
        public var ratePerSecond: Double
        /// Particles for every 100 points the finger moves, for a pan source.
        public var ratePer100Points: Double
        public var life: TimeInterval
        /// Points a second; the ends are kept in order.
        public var speed: ClosedRange<Double>
        /// The middle direction in radians, clockwise from the end direction.
        public var angle: Double
        /// The full width of the directions around it, in radians.
        public var spread: Double
        /// Points a second squared, downwards.
        public var gravity: Double
        /// Degrees a second, clockwise.
        public var spin: ClosedRange<Double>
        /// The share of life after which a particle fades; 1 or more never fades.
        public var fadeFrom: Double
        /// The size at the end of life, as a share of the look's.
        public var endScale: Double
        public var maxAlive: Int
        public var bursts: [Burst]

        public init(
            looks: [Look],
            source: Source,
            ratePerSecond: Double = 0,
            ratePer100Points: Double = 0,
            life: TimeInterval,
            speed: ClosedRange<Double> = 0 ... 0,
            angle: Double = 0,
            spread: Double = 0,
            gravity: Double = 0,
            spin: ClosedRange<Double> = 0 ... 0,
            fadeFrom: Double = 1,
            endScale: Double = 1,
            maxAlive: Int = Self.mostAlive,
            bursts: [Burst] = []
        ) {
            self.looks = looks
            self.source = source
            self.ratePerSecond = ratePerSecond
            self.ratePer100Points = ratePer100Points
            self.life = life
            self.speed = speed
            self.angle = angle
            self.spread = spread
            self.gravity = gravity
            self.spin = spin
            self.fadeFrom = fadeFrom
            self.endScale = endScale
            self.maxAlive = min(max(maxAlive, 0), Self.mostAlive)
            self.bursts = bursts
        }

        /// Where a particle is `age` after it left `start` at `speed` in `direction` (radians, clockwise from the end
        /// direction), with its opacity and scale, or nil once its life is over (`Emitter::place_at`).
        public func place(start: CGPoint, speed: Double, direction: Double, age: TimeInterval) -> ParticlePlace? {
            guard age < life, life > 0 else { return nil }
            let lived = age / life
            let x = start.x + speed * cos(direction) * age
            let y = start.y + speed * sin(direction) * age + gravity * age * age / 2
            let fadeFrom = min(fadeFrom, 1)
            let opacity = lived <= fadeFrom ? 1 : (1 - lived) / (1 - fadeFrom)
            return ParticlePlace(point: CGPoint(x: x, y: y), opacity: opacity, scale: 1 + (endScale - 1) * lived)
        }

        /// The farthest a particle can get from where it started, look included: how far past the node's frame
        /// particles may be drawn.
        var reach: CGFloat {
            let fastest = max(abs(speed.lowerBound), abs(speed.upperBound))
            let flight = fastest * life + abs(gravity) * life * life / 2
            let largest = looks.map(\.extent).max() ?? 0
            return CGFloat(flight) + largest * max(endScale, 1)
        }
    }

    struct ParticlePlace: Equatable {
        public let point: CGPoint
        public let opacity: Double
        public let scale: Double
    }
}

extension CustomMessageWidgetNode.Emitter.Look {
    /// The look's size in points.
    public var size: CGSize {
        switch self {
        case let .circle(diameter, _): CGSize(width: diameter, height: diameter)
        case let .rectangle(size, _): size
        case let .image(_, size): size
        }
    }

    var extent: CGFloat { hypot(size.width, size.height) / 2 }
}
