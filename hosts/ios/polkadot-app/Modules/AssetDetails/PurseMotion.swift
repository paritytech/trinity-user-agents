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
        /// The card's width once docked in the header.
        static let dockedWidth: CGFloat = 60
        /// Under Reduce Motion, where the in-page card starts fading into the
        /// header's thumbnail.
        static let handOverStart: CGFloat = 0.6

        /// How far the card has collapsed for a page scrolled `scrollOffset`
        /// points: 0 at rest, 1 once docked.
        static func progress(scrollOffset: CGFloat) -> CGFloat {
            clamp(scrollOffset / distance)
        }

        /// The scroll offset read back from where the card is, for systems
        /// without scroll geometry.
        static func scrollOffset(cardTop: CGFloat) -> CGFloat {
            max(0, restTop - cardTop)
        }

        /// Where the one card is drawn: it rides the page up from where it
        /// rests, and on the way moves to the header's centre and shrinks to
        /// its docked width, staying there however far the page scrolls on.
        static func floatingFrame(rest: CGRect, scrollOffset: CGFloat, dock: CGPoint) -> CGRect {
            let eased = smoothstep(progress(scrollOffset: scrollOffset))
            let riding = CGPoint(x: rest.midX, y: rest.midY - scrollOffset)
            let centre = CGPoint(
                x: riding.x + (dock.x - riding.x) * eased,
                y: riding.y + (dock.y - riding.y) * eased
            )
            let width = rest.width + (dockedWidth - rest.width) * eased
            let height = rest.width > 0 ? rest.height * width / rest.width : 0
            return CGRect(x: centre.x - width / 2, y: centre.y - height / 2, width: width, height: height)
        }

        static func balanceOpacity(_ progress: CGFloat) -> Double {
            Double(max(0, 1 - 1.8 * progress))
        }

        static func titleOpacity(_ progress: CGFloat) -> Double {
            Double(max(0, 1 - 2.5 * progress))
        }

        /// Under Reduce Motion nothing moves: the docked thumbnail fades in as
        /// the in-page card fades out.
        static func thumbnailOpacity(_ progress: CGFloat) -> Double {
            Double(smoothstep((progress - handOverStart) / (1 - handOverStart)))
        }

        static func cardOpacity(_ progress: CGFloat) -> Double {
            1 - thumbnailOpacity(progress)
        }
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
}
