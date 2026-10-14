import CoreGraphics
import Foundation

/// Every coin the strip draws, as a persistent object with a spring on each channel.
///
/// An arrangement change only moves targets, so a tap mid-flight re-aims rather than restarting,
/// and a coin keeps its identity across relayouts. Ported from `src/coin/coin-field.js`.
final class CoinageCoinField {
    /// One coin's channels. Every one of these is chased, not set.
    struct Channels {
        var centreX = CoinageSpring()
        var centreY = CoinageSpring()
        var lift = CoinageSpring()
        var height = CoinageSpring()
        var turn = CoinageSpring()
        var tilt = CoinageSpring()
        var thickness = CoinageSpring(value: 1)
        var wear = CoinageSpring()
        var luster = CoinageSpring()
        var calm = CoinageSpring()
    }

    /// Where one coin is heading.
    struct Target: Equatable {
        var centre: CGPoint
        var height: CGFloat
        var turn: CGFloat
        var tilt: CGFloat = 0
        var thickness: CGFloat
        var wear: CGFloat
        var luster: CGFloat
        var calm: CGFloat
        /// Toward the viewer.
        var lift: CGFloat = 0
    }

    struct Member {
        let coin: CoinageScene.Coin
        /// Scatters this coin's pits and streaks. Taken from its identity, so a coin keeps the same
        /// face across frames and across relayouts without anything being stored.
        let seed: Float
        var channels: Channels
        var target: Target
        /// Staggers this coin's start across an arrangement change, in display order.
        var hold: CGFloat
    }

    private(set) var members: [Member] = []
    private var index: [String: Int] = [:]

    /// True while any coin is still travelling, which is what decides whether to draw a frame.
    private(set) var isMoving = false

    static func seed(for id: String) -> Float {
        let hashed = id.unicodeScalars.reduce(UInt64(0x9E37_79B9_7F4A_7C15)) {
            ($0 &* 31) &+ UInt64($1.value)
        }

        return Float(hashed % 100_000) / 1_000
    }

    /// Beyond this a coin counts as in flight, which caps its mesh detail and holds back its

    /// Which end of the field sets off first.
    ///
    /// Coins leave in display order and return in reverse, so in both directions the ones with
    /// furthest to go start first. Spreading into the grid the far end has the long haul, and
    /// gathering back into the strip it is the near end; taking the same end each way left the long
    /// travellers setting off last and arriving well after everything else had settled.
    enum Stagger {
        case fromFront
        case fromBack

        func position(of order: Int, of count: Int) -> Int {
            switch self {
            case .fromFront: order
            case .fromBack: count - 1 - order
            }
        }
    }

    /// Re-aims every coin. A coin already in the field keeps where it has got to and simply gets a
    /// new destination, so a tap mid-flight re-aims rather than restarting. A coin new to the field
    /// arrives from off the right edge at a fraction of its size and fully worn, and settles in.
    func retarget(
        _ targets: [(coin: CoinageScene.Coin, target: Target)],
        spawningFrom edge: CGFloat,
        stagger: Stagger
    ) {
        var updated: [Member] = []
        var lookup: [String: Int] = [:]
        let count = max(targets.count, 1)

        for (order, entry) in targets.enumerated() {
            let hold = CGFloat(stagger.position(of: order, of: count))
                / CGFloat(count) * CoinageSpring.stagger

            if let existing = index[entry.coin.id], existing < members.count {
                var member = members[existing]
                member.target = entry.target
                member.hold = hold
                updated.append(member)
            } else {
                updated.append(
                    Member(
                        coin: entry.coin,
                        seed: Self.seed(for: entry.coin.id),
                        channels: seeded(for: entry.target, edge: edge),
                        target: entry.target,
                        hold: hold
                    )
                )
            }

            lookup[entry.coin.id] = updated.count - 1
        }

        members = updated
        index = lookup
        isMoving = true
    }

    /// Advances every spring. Returns whether anything is still moving, so a settled field can stop
    /// asking for frames.
    @discardableResult
    func advance(by seconds: CGFloat) -> Bool {
        var moving = false

        for position in members.indices {
            let step = max(seconds - members[position].hold, 0)
            members[position].hold = max(members[position].hold - seconds, 0)

            guard step > 0 else {
                moving = true
                continue
            }

            moving = advance(&members[position], by: step) || moving
        }

        isMoving = moving

        return moving
    }

    /// How far a coin still has to travel, which decides how much luster it has picked up.
    func distanceToTarget(_ member: Member) -> CGFloat {
        let across = member.channels.centreX.value - member.target.centre.x
        let down = member.channels.centreY.value - member.target.centre.y

        return (across * across + down * down).squareRoot()
    }
}

// MARK: - Stepping

private extension CoinageCoinField {
    /// Where a coin new to the field starts: off the right edge, small, and as traceable as a coin
    /// can be, so arriving money visibly settles into the strip and then clears.
    static let spawnOffset: CGFloat = 30
    static let spawnScale: CGFloat = 0.4

    func seeded(for target: Target, edge: CGFloat) -> Channels {
        var channels = Channels()
        channels.centreX = CoinageSpring(value: edge + Self.spawnOffset)
        channels.centreY = CoinageSpring(value: target.centre.y)
        channels.height = CoinageSpring(value: target.height * Self.spawnScale)
        channels.turn = CoinageSpring(value: target.turn)
        channels.tilt = CoinageSpring(value: target.tilt)
        channels.thickness = CoinageSpring(value: target.thickness)
        channels.wear = CoinageSpring(value: CoinageWear.unknown)
        channels.calm = CoinageSpring(value: target.calm)

        return channels
    }

    func advance(_ member: inout Member, by seconds: CGFloat) -> Bool {
        let target = member.target

        member.channels.centreX.step(toward: target.centre.x, seconds: seconds, omega: CoinageSpring.Omega.position)
        member.channels.centreY.step(toward: target.centre.y, seconds: seconds, omega: CoinageSpring.Omega.position)
        member.channels.height.step(toward: target.height, seconds: seconds, omega: CoinageSpring.Omega.position)
        member.channels.lift.step(toward: target.lift, seconds: seconds, omega: CoinageSpring.Omega.position)
        member.channels.turn.step(toward: target.turn, seconds: seconds, omega: CoinageSpring.Omega.rotation)
        member.channels.tilt.step(toward: target.tilt, seconds: seconds, omega: CoinageSpring.Omega.rotation)
        member.channels.thickness.step(toward: target.thickness, seconds: seconds, omega: CoinageSpring.Omega.thickness)
        member.channels.calm.step(toward: target.calm, seconds: seconds, omega: CoinageSpring.Omega.thickness)
        member.channels.luster.step(toward: target.luster, seconds: seconds, omega: CoinageSpring.Omega.luster)
        member.channels.wear.step(toward: target.wear, seconds: seconds, omega: CoinageSpring.Omega.wear)

        return !settled(member)
    }

    func settled(_ member: Member) -> Bool {
        let target = member.target
        let channels = member.channels

        return channels.centreX.isSettled(at: target.centre.x)
            && channels.centreY.isSettled(at: target.centre.y)
            && channels.height.isSettled(at: target.height)
            && channels.turn.isSettled(at: target.turn)
            && channels.tilt.isSettled(at: target.tilt)
            && channels.thickness.isSettled(at: target.thickness)
            && channels.calm.isSettled(at: target.calm)
            && channels.lift.isSettled(at: target.lift)
            && channels.luster.isSettled(at: target.luster)
            && channels.wear.isSettled(at: target.wear)
    }
}
