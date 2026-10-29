import CoreGraphics

/// Classifies the end of a horizontal drag on the bar as a selection, a fold, an unfold or a settle.
enum TabBarFoldDecision: Equatable {
    case select
    case fold
    case unfold
    case settle

    static func decide(
        velocityX: CGFloat,
        translationX: CGFloat,
        isFolded: Bool,
        foldDistance: CGFloat
    ) -> TabBarFoldDecision {
        isFolded
            ? decideFromFolded(velocityX: velocityX, translationX: translationX, foldDistance: foldDistance)
            : decideFromShown(velocityX: velocityX, translationX: translationX)
    }
}

private extension TabBarFoldDecision {
    /// Horizontal speed, in points per second, past which a drag reads as a flick.
    static let flickVelocity: CGFloat = 800
    /// Distance, in points, a flick has to cover before it counts as one rather than a jitter.
    static let flickMinTravel: CGFloat = 20
    /// Movement, in points, a drag on the folded bar may have and still read as a tap.
    static let tapSlop: CGFloat = 10
    /// Share of the fold distance a slow drag has to cover to settle at the unfolded end.
    static let settleFraction: CGFloat = 0.3

    static func decideFromShown(velocityX: CGFloat, translationX: CGFloat) -> TabBarFoldDecision {
        let flicked = velocityX < -flickVelocity
        let travelled = translationX < -flickMinTravel
        return flicked && travelled ? .fold : .select
    }

    static func decideFromFolded(
        velocityX: CGFloat,
        translationX: CGFloat,
        foldDistance: CGFloat
    ) -> TabBarFoldDecision {
        guard abs(translationX) > tapSlop else {
            return .unfold
        }

        let flicked = velocityX > flickVelocity
        let travelled = translationX > flickMinTravel

        guard !(flicked && travelled) else {
            return .unfold
        }

        let settleThreshold = abs(foldDistance) * settleFraction
        return translationX > settleThreshold ? .unfold : .settle
    }
}
