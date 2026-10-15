import Foundation
import TrUAPIHost
@testable import polkadot_app

final class MockConfirmationPresenter: TrUAPIConfirmationPresenting, @unchecked Sendable {
    var receivedReview: UserConfirmationReview?
    var receivedRequesterName: String?
    var verdictToReturn: Bool = true
    var permissionDecisionToReturn: TrUAPIPermissionDecision = .allowAlways
    /// When set, `confirmPermission` throws it, as a prompt that no screen answers.
    var permissionError: Error?

    func confirm(review: UserConfirmationReview, from requesterName: String) async -> Bool {
        receivedReview = review
        receivedRequesterName = requesterName
        return verdictToReturn
    }

    func confirmPermission(
        review: UserConfirmationReview,
        from requesterName: String
    ) async throws -> TrUAPIPermissionDecision {
        receivedReview = review
        receivedRequesterName = requesterName
        if let permissionError { throw permissionError }
        return permissionDecisionToReturn
    }
}
