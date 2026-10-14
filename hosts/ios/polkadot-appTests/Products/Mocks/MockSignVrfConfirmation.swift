import Foundation

@testable import polkadot_app

final class MockSignVrfConfirmation: SignVrfConfirming, @unchecked Sendable {
    var decision: SignVrfDecision = .approved
    private(set) var requests: [SignVrfConfirmationRequest] = []

    func confirm(_ request: SignVrfConfirmationRequest) async -> SignVrfDecision {
        requests.append(request)
        return decision
    }
}
