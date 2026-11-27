import TrUAPIHost
@testable import polkadot_app

/// Answers with an outcome no other path gives, so a test can tell its answer arrived.
@MainActor
final class StubCoveredCard: ExpandedCardCovering {
    private(set) var requests: [Bool] = []

    func setCardCovered(_ covered: Bool) -> ExpandedCardCoverOutcome {
        requests.append(covered)
        return .userMoving
    }
}
