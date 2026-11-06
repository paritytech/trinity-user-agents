import CoreGraphics

extension CGFloat {
    /// The value clamped to 0...1, eased with the cubic smoothstep `t * t * (3 - 2t)`.
    var smoothstepped: CGFloat {
        let clamped = clampedToUnit
        return clamped * clamped * (3 - 2 * clamped)
    }

    /// A saturating curve for overdrag, defined for values of at least 0. It starts with slope `slope` at 0 and
    /// approaches `limit` as the value grows.
    func rubberBanded(limit: CGFloat, slope: CGFloat = 1) -> CGFloat {
        limit * (1 - 1 / (1 + (slope / limit) * self))
    }

    /// The value clamped to 0...1.
    var clampedToUnit: CGFloat {
        Swift.min(Swift.max(self, 0), 1)
    }
}
