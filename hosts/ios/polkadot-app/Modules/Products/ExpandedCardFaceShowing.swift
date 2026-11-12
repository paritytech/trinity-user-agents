import TrUAPIHost

/// `shown: false` means the page covers the face; the face itself never moves.
@MainActor
protocol ExpandedCardFaceShowing: AnyObject {
    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome
}
