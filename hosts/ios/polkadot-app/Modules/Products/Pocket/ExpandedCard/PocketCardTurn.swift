import Foundation
import Observation
import SwiftUI
import enum TrUAPIHost.PocketCardSide
import UIKit

/// Steps a turn frame by frame until `step` answers false. `step` is handed the seconds since the last frame.
@MainActor
protocol PocketCardTurnFrames: AnyObject {
    func start(_ step: @escaping (Double) -> Bool)
}

/// How far the opened card is turned over. `angle` is in degrees: 0 is the front, 180 the back.
///
/// The turn is stepped here rather than left to a SwiftUI animation, because what the card does next depends on
/// where it is mid-turn: a drag holds it there, and a reversal goes back the short way from there.
@Observable
@MainActor
final class PocketCardTurn {
    private(set) var angle = 0.0
    private(set) var hasBack = false
    /// The user holds the card from the start of a drag until its settle ends.
    private(set) var isUserMoving = false

    /// Where the card is going: the end of a turn under way, else where it is.
    @ObservationIgnored private var heading = 0.0
    /// The face up when the current drag began: a swipe turns the card at most one flip from it.
    @ObservationIgnored private var dragFace = 0.0
    @ObservationIgnored private var isHoming = false
    @ObservationIgnored private var settle: Settle?
    private let frames: any PocketCardTurnFrames
    private let reduceMotion: @MainActor () -> Bool

    init(
        frames: (any PocketCardTurnFrames)? = nil,
        reduceMotion: @escaping @MainActor () -> Bool = { UIAccessibility.isReduceMotionEnabled }
    ) {
        self.frames = frames ?? DisplayLinkFrames()
        self.reduceMotion = reduceMotion
    }

    var canTurn: Bool { hasBack && !isHoming }

    var shownSide: PocketCardSide { Self.shownSide(at: angle) }

    /// 0 while the card lies flat, 1 when it is seen side-on.
    var sideOn: Double { abs(sine) }

    /// Smaller while side-on, so perspective never draws the near side taller than the card.
    var scale: Double { 1 - Self.sideOnShrink * sideOn }

    /// Whether the card's own left side, rather than its right, is the one turned toward the viewer.
    var isLeftSideNearer: Bool { sine > 0 }

    /// How far sideways a layer `depth` points behind the card's middle is drawn at this angle.
    func depthOffset(_ depth: Double) -> Double { -depth * sine }

    /// The rotation `side` is drawn at, kept within ±180° so the side up is never drawn mirrored.
    func layerRotation(_ side: PocketCardSide) -> Double {
        let turned = side == .front ? angle : angle + Self.halfTurn
        let wrapped = (turned + Self.halfTurn).truncatingRemainder(dividingBy: Self.fullTurn)
        return (wrapped < 0 ? wrapped + Self.fullTurn : wrapped) - Self.halfTurn
    }

    func backChanged(available: Bool) {
        hasBack = available
        guard !available else { return }

        settle = nil
        isUserMoving = false
        isHoming = false
        angle = Self.nearest(angle, step: Self.fullTurn)
        heading = angle
    }

    /// Stops a settle under way: the finger outranks it. A card turning home is not stopped.
    func beginDrag() {
        guard !isHoming else { return }

        isUserMoving = true
        dragFace = Self.nearest(heading, step: Self.halfTurn)
        settle = nil
    }

    func drag(by points: Double, width: Double) {
        guard isUserMoving, !isHoming else { return }

        angle = withinOneFlip(angle + points * Self.halfTurn / width)
        heading = angle
    }

    /// Lands on the face the release heads for, never more than one flip from the face the drag began on.
    func release(velocity pointsPerSecond: Double, width: Double) {
        guard isUserMoving, !isHoming else { return }

        // Capped so a hard flick still settles on the next face instead of swinging past it.
        let velocity = min(max(pointsPerSecond * Self.halfTurn / width, -Self.maxSettleSpeed), Self.maxSettleSpeed)
        let rest = withinOneFlip(Self.nearest(angle + velocity * Self.flingProjection, step: Self.halfTurn))
        start(Settle(start: angle, target: rest, velocity: velocity, spring: Self.userSpring, owner: .user))
    }

    /// Turns to `side`, compared with where the card is going rather than where it is mid-turn.
    func turn(to side: PocketCardSide) {
        guard !isUserMoving, !isHoming else { return }

        if side != Self.shownSide(at: heading) {
            turnOver()
        } else if angle != heading, settle == nil {
            start(Settle(start: angle, target: heading, velocity: 0, spring: Self.turnSpring, owner: .page))
        }
    }

    /// Turns to the other side of where the card is going, so it reverses a turn under way the short way. From rest
    /// it turns the way a right-to-left swipe does, the one a right thumb makes.
    func turnOver() {
        guard !isUserMoving, !isHoming else { return }

        let rightToLeft = Self.nearest(heading, step: Self.halfTurn) - Self.halfTurn
        let leftToRight = rightToLeft + Self.fullTurn
        let target = abs(leftToRight - angle) < abs(rightToLeft - angle) ? leftToRight : rightToLeft
        start(Settle(start: angle, target: target, velocity: 0, spring: Self.turnSpring, owner: .page))
    }

    /// Turns the card back to its front for closing, which a drag cannot interrupt.
    func turnHome() {
        isHoming = true
        isUserMoving = false
        start(Settle(
            start: angle,
            target: Self.nearest(angle, step: Self.fullTurn),
            velocity: 0,
            spring: Self.userSpring,
            owner: .home
        ))
    }
}

private extension PocketCardTurn {
    static let halfTurn = 180.0
    static let fullTurn = 360.0
    static let flingProjection = 0.3
    static let maxSettleSpeed = 720.0
    // Measured on Android: side-on, the near side is drawn about 1.23 times the card's height.
    static let sideOnShrink = 0.2
    static let userSpring = Spring(duration: 0.4, bounce: 0.15)
    // A turn nobody's finger drives is followed by eye, so it runs 25% longer.
    static let turnSpring = Spring(duration: 0.5, bounce: 0.15)

    enum Owner {
        case user
        case page
        case home
    }

    struct Settle {
        let start: Double
        let target: Double
        let velocity: Double
        let spring: Spring
        let owner: Owner
        var elapsed = 0.0
    }

    var sine: Double { sin(angle * .pi / 180) }

    static func shownSide(at angle: Double) -> PocketCardSide {
        cos(angle * .pi / 180) >= 0 ? .front : .back
    }

    /// Ties round up, as Android's do, so both hosts turn home the same way.
    static func nearest(_ degrees: Double, step: Double) -> Double {
        (degrees / step + 0.5).rounded(.down) * step
    }

    func withinOneFlip(_ degrees: Double) -> Double {
        min(max(degrees, dragFace - Self.halfTurn), dragFace + Self.halfTurn)
    }

    func start(_ newSettle: Settle) {
        heading = newSettle.target
        guard !reduceMotion(), newSettle.target != angle || newSettle.velocity != 0 else {
            settle = nil
            angle = newSettle.target
            end(newSettle.owner)
            return
        }

        settle = newSettle
        frames.start { [weak self] seconds in self?.step(by: seconds) ?? false }
    }

    func step(by seconds: Double) -> Bool {
        guard var running = settle else { return false }

        running.elapsed += seconds
        guard running.elapsed < running.spring.settlingDuration else {
            settle = nil
            angle = running.target
            end(running.owner)
            return false
        }

        settle = running
        angle = running.spring.value(
            fromValue: running.start,
            toValue: running.target,
            initialVelocity: running.velocity,
            time: running.elapsed
        )
        return true
    }

    func end(_ owner: Owner) {
        switch owner {
        case .user: isUserMoving = false
        case .home: isHoming = false
        case .page: break
        }
    }
}

/// Steps a turn once per screen refresh.
@MainActor
private final class DisplayLinkFrames: NSObject, PocketCardTurnFrames {
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

    /// Steps by the time that actually passed, so a dropped frame does not slow the turn.
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
