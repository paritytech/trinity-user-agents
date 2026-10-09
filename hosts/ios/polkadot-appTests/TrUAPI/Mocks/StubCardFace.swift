import TrUAPIHost
@testable import polkadot_app

/// Answers every request with an outcome no other path gives, so a test can
/// tell its answer arrived.
@MainActor
final class StubCardFace: ExpandedCardFaceShowing {
    private(set) var requests: [Bool] = []

    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        requests.append(shown)
        return .userMoving
    }
}
