import TrUAPIHost
@testable import polkadot_app

/// Answers with an outcome no other path gives, so a test can tell its answer arrived.
@MainActor
final class StubCardFace: ExpandedCardFaceShowing {
    private(set) var requests: [Bool] = []
    private(set) var backRequests: [Bool] = []

    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        requests.append(shown)
        return .userMoving
    }

    func setBackShown(_ shown: Bool) -> ExpandedCardTurnOutcome {
        backRequests.append(shown)
        return .noBack
    }
}
