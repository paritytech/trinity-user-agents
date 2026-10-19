import CoreGraphics
import Foundation

/// The Purse's motion, in one place so the design's numbers are read and tuned
/// together. Durations and delays are in seconds.
enum PurseMotion {
    /// The card shrinking into the header as the Purse scrolls.
    enum Collapse {
        /// How far the page scrolls while the card shrinks.
        static let distance: CGFloat = 150
        /// The card's top in the wallet's viewport before any scroll.
        static let restTop: CGFloat = 16
        /// The card's width once docked in the header, against its full width.
        static let dockedWidth: CGFloat = 60
        static let fullWidth: CGFloat = 366
        /// Where the in-page card hands over to the header's thumbnail.
        static let handOverStart: CGFloat = 0.6

        /// How far the card has collapsed: 0 at rest, 1 once docked.
        static func progress(cardTop: CGFloat) -> CGFloat {
            clamp((restTop - cardTop) / distance)
        }

        static func cardScale(_ progress: CGFloat) -> CGFloat {
            1 + (dockedWidth / fullWidth - 1) * smoothstep(progress)
        }

        static func balanceOpacity(_ progress: CGFloat) -> Double {
            Double(max(0, 1 - 1.8 * progress))
        }

        static func titleOpacity(_ progress: CGFloat) -> Double {
            Double(max(0, 1 - 2.5 * progress))
        }

        /// The header's thumbnail fades in as the in-page card fades out.
        static func thumbnailOpacity(_ progress: CGFloat) -> Double {
            Double(smoothstep((progress - handOverStart) / (1 - handOverStart)))
        }

        static func cardOpacity(_ progress: CGFloat) -> Double {
            1 - thumbnailOpacity(progress)
        }
    }

    /// A row's icon turning from a spinner into what happened.
    enum Completion {
        static let loaderOut: TimeInterval = 0.24
        static let shaftDelay: TimeInterval = 0.22
        static let shaft: TimeInterval = 0.18
        static let legsDelay: TimeInterval = 0.4
        static let legs: TimeInterval = 0.18
        static let shrinkDelay: TimeInterval = 0.75
        static let shrink: TimeInterval = 0.3
        static let outlineDelay: TimeInterval = 0.88
        static let outline: TimeInterval = 0.45
        static let dotDelay: TimeInterval = 1.23
        /// How big the arrow is drawn before it shrinks into the card's corner.
        static let arrowStartScale: CGFloat = 1 / 0.424_26
        static let crossFirstDelay: TimeInterval = 0.22
        static let crossSecondDelay: TimeInterval = 0.38
        static let crossStroke: TimeInterval = 0.2
        /// After this an ended row draws at rest.
        static let total: TimeInterval = 1.6
    }

    /// A finished session arriving at the top of Today.
    enum Arrival {
        static let delay: TimeInterval = 0.26
        static let duration: TimeInterval = 0.56
        static let offset: CGFloat = -10
        static let blur: CGFloat = 4
        /// How long a session counts as just finished.
        static let window: TimeInterval = 3
    }

    /// The in-progress dock.
    enum Dock {
        static let spring: TimeInterval = 0.45
        static let ringMorph: TimeInterval = 0.52
        static let ringStartRadius: CGFloat = 10.8
        static let ringEndRadius: CGFloat = 22.8
        static let numberDelay: TimeInterval = 0.26
        static let numberIn: TimeInterval = 0.38
        static let numberSlide: TimeInterval = 0.42
        /// Every period the count turns into an up chevron for a moment,
        /// hinting that the dock opens.
        static let hintPeriod: TimeInterval = 10
        static let hintStart: TimeInterval = 7.2
        static let hintEnd: TimeInterval = 9.4
        static let spinnerPeriod: TimeInterval = 0.9
    }

    /// The Total panel.
    enum Balance {
        static let countUp: TimeInterval = 0.65
        static let stripePeriod: TimeInterval = 0.7
    }

    /// Hermite smoothstep over 0...1.
    static func smoothstep(_ value: CGFloat) -> CGFloat {
        let clamped = clamp(value)
        return clamped * clamped * (3 - 2 * clamped)
    }

    static func clamp(_ value: CGFloat) -> CGFloat {
        min(max(value, 0), 1)
    }

    /// Whether the dock's count shows its hint chevron `elapsed` seconds into
    /// its cycle.
    static func showsHint(elapsed: TimeInterval) -> Bool {
        let phase = elapsed.truncatingRemainder(dividingBy: Dock.hintPeriod)
        return phase >= Dock.hintStart && phase < Dock.hintEnd
    }
}
