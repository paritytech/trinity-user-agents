import Foundation

@testable import polkadot_app

final class MockCreateProofConfirmation: CreateProofConfirming, @unchecked Sendable {
    var decision: CreateProofDecision = .approved
    private(set) var requests: [CreateProofConfirmationRequest] = []

    func confirm(_ request: CreateProofConfirmationRequest) async -> CreateProofDecision {
        requests.append(request)
        return decision
    }
}
