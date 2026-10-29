import TrUAPIHost

/// Shows or hides the face of the Pocket card a page is opened under, at that page's request.
@MainActor
protocol ExpandedCardFaceShowing {
    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome
}
