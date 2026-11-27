import TrUAPIHost

/// `covered: true` means the page covers the face; the face itself never moves.
@MainActor
protocol ExpandedCardCovering: AnyObject {
    func setCardCovered(_ covered: Bool) -> ExpandedCardCoverOutcome
}
