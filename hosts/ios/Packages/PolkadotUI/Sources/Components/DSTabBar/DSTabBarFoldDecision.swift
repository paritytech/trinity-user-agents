import CoreGraphics

enum DSTabBarFoldDecision: Equatable {
    case select
    case fold
    case unfold
    case settle

    static func decide(
        velocityX: CGFloat,
        translationX: CGFloat,
        isFolded: Bool,
        foldDistance: CGFloat
    ) -> DSTabBarFoldDecision {
        isFolded
            ? decideFromFolded(velocityX: velocityX, translationX: translationX, foldDistance: foldDistance)
            : decideFromShown(velocityX: velocityX, translationX: translationX)
    }
}

private extension DSTabBarFoldDecision {
    static func decideFromShown(velocityX: CGFloat, translationX: CGFloat) -> DSTabBarFoldDecision {
        let flicked = velocityX < -DSTabBarMetrics.foldFlickVelocity
        let travelled = translationX < -DSTabBarMetrics.foldFlickMinTravel
        return flicked && travelled ? .fold : .select
    }

    static func decideFromFolded(
        velocityX: CGFloat,
        translationX: CGFloat,
        foldDistance: CGFloat
    ) -> DSTabBarFoldDecision {
        guard abs(translationX) > DSTabBarMetrics.foldTapSlop else {
            return .unfold
        }

        let flicked = velocityX > DSTabBarMetrics.foldFlickVelocity
        let travelled = translationX > DSTabBarMetrics.foldFlickMinTravel

        guard !(flicked && travelled) else {
            return .unfold
        }

        let settleThreshold = abs(foldDistance) * DSTabBarMetrics.foldSettleFraction
        return translationX > settleThreshold ? .unfold : .settle
    }
}
