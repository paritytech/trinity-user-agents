import Foundation
import TrUAPIHost
@testable import polkadot_app

final class MockConfirmationPresenter: TrUAPIConfirmationPresenting, @unchecked Sendable {
    var receivedReview: UserConfirmationReview?
    var receivedRequesterName: String?
    var receivedRoute: RequestRoute?
    var verdictToReturn: Bool = true
    var permissionDecisionToReturn: TrUAPIPermissionDecision = .allowAlways

    func confirm(review: UserConfirmationReview, from requesterName: String, route: RequestRoute) async -> Bool {
        receivedReview = review
        receivedRequesterName = requesterName
        receivedRoute = route
        return verdictToReturn
    }

    func confirmPermission(
        review: UserConfirmationReview,
        from requesterName: String,
        route: RequestRoute
    ) async -> TrUAPIPermissionDecision {
        receivedReview = review
        receivedRequesterName = requesterName
        receivedRoute = route
        return permissionDecisionToReturn
    }
}
