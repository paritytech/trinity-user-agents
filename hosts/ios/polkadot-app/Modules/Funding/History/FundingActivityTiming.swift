import Foundation

/// The timings of the funding activity rows.
enum FundingActivityTiming {
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
}
