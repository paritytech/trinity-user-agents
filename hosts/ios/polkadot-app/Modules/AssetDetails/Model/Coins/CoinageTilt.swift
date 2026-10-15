import CoreMotion
import Foundation
import PolkadotUI
import simd

/// Turns the studio with the phone, so the light behaves as though it were fixed in the room.
///
/// The environment the coins are lit against is defined around the coin's own space, so with the
/// studio held still the highlight sits in the same place however the phone is held. Feeding the
/// device's own tilt back in turns the studio the other way, and a coin catches the light as you
/// tilt it, the way a real one does.
///
/// A sideways movement rolls the studio about the axis you are looking down; leaning the phone back
/// pitches it. That first part is a deliberate choice, not physics, and it is the fix for a light
/// that responded to one direction of tilt and not the other.
///
/// Turning the studio the way the phone really turned puts a sideways movement about the screen's
/// long axis, which slides the reflection horizontally. This studio is a room: bright above, dark
/// below, and nearly even from side to side. Measured across the whole face of a coin, forty
/// degrees of that turn changes what it reflects by 0.31 one way and by 0.10 the other — a dead
/// side, and correcting the sense of the turn only moved the dead side to the other hand. Rolling
/// the room about the view axis changes it by 0.18 either way, symmetric by construction: a roll
/// about the axis the coin faces cannot care which way it went.
///
/// It is also what the eye expects. The head does not roll with the phone, so relative to the
/// screen the room is what rolls, and a coin's highlight sweeping around its face is the thing that
/// reads as catching the light. Exactly right for a phone held up to look at, a simplification for
/// one flat on a table, and the only movement this environment has the contrast to show.
///
/// Device motion needs no permission prompt: `CMMotionManager` reads the accelerometer and gyro
/// directly. Only `CMMotionActivityManager`, which classifies walking and driving, requires
/// `NSMotionUsageDescription` and asks the user. This reads attitude only.
///
/// Readings come from the app's shared source rather than a manager of this view's own. The card
/// effects read the same one, and this screen shows both at once, so a manager each meant two
/// copies of the same sensor pipeline. Watching it is a subscription: dropping it releases the
/// sensor if nothing else is watching, and cannot switch off somebody else's.
final class CoinageTilt {
    /// Where the studio is turned to: an axis scaled by the angle turned about it, in radians.
    ///
    /// One vector rather than a pair of angles, because a rotation about a slanted axis is not the
    /// sum of two rotations about the axes either side of it, and the phone rotates about whatever
    /// axis the wrist chose.
    typealias Turn = SIMD3<Double>

    /// How far the studio turns at the ends of the travel. A whole rotation would swing the
    /// highlight right around the coin, which reads as a spinning lamp rather than a coin in a hand.
    static let travel: Double = 0.9

    /// The tilt that reaches the ends of the travel. Deliberately short of a right angle: nobody
    /// turns a phone ninety degrees to look at it, and the whole range should be within a normal
    /// movement of the wrist.
    static let range: Double = .pi / 4

    /// How quickly a phone that is being held still becomes the new neutral.
    ///
    /// Zero offset is where the studio sits as it was calibrated: square to the screen, which is
    /// where the coins have most contrast. Any fixed idea of how a phone is held is wrong for
    /// somebody, so neutral has to follow.
    ///
    /// It follows only while the phone is still. An average over the last few seconds cannot tell
    /// a deliberate tilt from a new resting position, so it took both: a two second tilt dragged
    /// neutral thirteen degrees, and putting the phone back where it started left a dozen degrees
    /// of light that should not have been there. Worse, that bias ate one side of the travel, so
    /// tilting one way did nothing while the other still worked.
    static let settle: TimeInterval = 1.0

    /// Above this the phone is being moved rather than held, and neutral waits.
    ///
    /// Well clear of sensor noise and well under a deliberate tilt, which covers tens of degrees in
    /// well under a second.
    static let stillSpeed: Double = 0.25

    /// How long the phone has to be still before it counts as put down somewhere new.
    ///
    /// A hand pauses at the end of a gesture before bringing the phone back, and without this that
    /// pause starts moving neutral: the tilt is then partly taken as the new resting place, and
    /// coming back leaves the light off square at a position that has not changed.
    static let dwell: TimeInterval = 0.4

    /// Chases the device rather than tracking it exactly: raw attitude is noisy enough to make a
    /// specular highlight shimmer while the phone is held still.
    static let smoothing: Double = 8

    private static let epsilon: Double = 0.0008

    private let motion: DeviceMotionObservable
    private var subscription: DeviceMotionToken?
    private var neutral: Pose?
    private var previous: Pose?
    private var stillFor: TimeInterval = 0
    private var target = Turn()

    private(set) var turn = Turn()

    init(motion: DeviceMotionObservable = DeviceMotionSource.shared) {
        self.motion = motion
    }

    /// Called when the phone has moved enough that the studio needs to catch up. The view pauses
    /// itself whenever nothing is moving, so without a push from outside it would never look at the
    /// attitude again and the light would stay where the field last settled.
    var onMove: (() -> Void)?

    func start() {
        guard subscription == nil else { return }

        let interval = motion.updateInterval
        subscription = motion.observe { [weak self] gravity in
            self?.absorb(Pose(gravity: gravity), after: interval)
        }
    }

    func stop() {
        guard subscription != nil else { return }

        subscription = nil
        neutral = nil
        previous = nil
        stillFor = 0
        onMove = nil
    }

    /// Eases toward the latest turn. Returns whether the studio is still catching up, so a field at
    /// rest under a still hand goes back to sleep.
    @discardableResult
    func advance(by seconds: CGFloat) -> Bool {
        guard !isSettled else { return false }

        turn += (target - turn) * min(Double(seconds) * Self.smoothing, 1)

        return true
    }

    /// Folds one reading into the neutral and works out where the studio should sit against it.
    func absorb(_ pose: Pose, after seconds: TimeInterval) {
        var settled = neutral ?? pose

        if let previous, neutral != nil {
            stillFor = previous.angle(to: pose) / max(seconds, 0.0001) < Self.stillSpeed
                ? stillFor + seconds
                : 0

            // Put down somewhere new, so wherever it is becomes square. Still being moved, or only
            // paused mid-gesture, so neutral waits and the movement reads in full.
            if stillFor >= Self.dwell {
                settled = settled.blended(toward: pose, by: min(seconds / Self.settle, 1))
            }
        } else {
            // The first reading is neutral outright, so the light starts square rather than
            // drifting in from wherever the phone happened to be picked up.
            settled = pose
        }

        neutral = settled
        previous = pose
        target = settled.turn(to: pose)

        if !isSettled { onMove?() }
    }

    /// Scales a turn of `angle` into the travel, so the ends of the wrist's range reach the ends of
    /// the studio's and no tilt can push it further.
    static func scaled(_ angle: Double) -> Double {
        min(max(angle, -range), range) / range * travel
    }

    /// The two turns as one, in the order they are felt: the room rolls about the view axis, and
    /// the whole of it is then pitched by however far the phone is leaned back.
    ///
    /// Composed as rotations rather than added as a vector. Turns about different axes do not add,
    /// and these two are large enough at the ends of the travel for the difference to show.
    static func composed(pitch: Double, roll: Double) -> Turn {
        let turn = simd_quatd(angle: pitch, axis: SIMD3(1, 0, 0))
            * simd_quatd(angle: roll, axis: SIMD3(0, 0, 1))

        return abs(turn.angle) < 1e-9 ? Turn() : turn.axis * turn.angle
    }
}

// MARK: - Settling

private extension CoinageTilt {
    var isSettled: Bool {
        abs(target - turn).max() <= Self.epsilon
    }
}

// MARK: - Readings

extension CoinageTilt {
    /// One reading: which way is down, in the phone's own frame.
    ///
    /// Gravity is all the studio needs and all it can have. It fixes the phone's orientation up to a
    /// turn about the vertical, which is exactly the turn a room's lighting does not care about,
    /// and unlike integrated attitude it never drifts.
    struct Pose: Equatable {
        let down: SIMD3<Double>

        init(gravity: CMAcceleration) {
            self.init(down: SIMD3(gravity.x, gravity.y, gravity.z))
        }

        init(down: SIMD3<Double>) {
            self.down = down / max(length(down), 1e-9)
        }

        /// How far the right edge has dipped, and how far the screen has leaned back from vertical.
        ///
        /// Each against everything left over rather than against one other component. Anything of
        /// the form `atan2(x, y)` between two components that both shrink is a bearing, and a
        /// bearing is ill-conditioned wherever the phone is leaned well back — the way it is held
        /// to look at something. A five degree roll once read as five degrees upright, twenty-seven
        /// leaned back and a hundred and seventy past flat.
        var sideways: Double { atan2(down.x, hypot(down.y, down.z)) }
        var lean: Double { atan2(-down.z, hypot(down.x, down.y)) }

        /// How far the phone turned between two readings, whatever axis it turned about. Used to
        /// tell a phone being moved from one being held still, where the axis does not matter.
        ///
        /// Against the dot product rather than from it alone: `acos` loses most of its precision
        /// for the small angles between consecutive readings, which is where this is asked most.
        func angle(to other: Pose) -> Double {
            atan2(length(cross(down, other.down)), dot(down, other.down))
        }

        /// Where the studio sits with the phone here, against this reading as square.
        func turn(to other: Pose) -> Turn {
            CoinageTilt.composed(
                pitch: CoinageTilt.scaled(other.lean - lean),
                roll: CoinageTilt.scaled(other.sideways - sideways)
            )
        }

        /// Eases toward another reading, staying a direction rather than shrinking toward the
        /// middle: neutral is which way is down, and a blend of two directions has no length of
        /// its own to keep.
        func blended(toward other: Pose, by blend: Double) -> Pose {
            Pose(down: down + (other.down - down) * blend)
        }
    }
}
