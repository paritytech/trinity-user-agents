import Foundation
import TrUAPIHost

/// Opens a funding session on the host's own behalf, as the CASH card does.
/// Answers the session id, or `nil` when the user dismissed the overlay.
protocol FundingOpening: Sendable {
    func openFunding(direction: FundingDirection) async throws -> String?
}

enum FundingOpeningError: Error {
    case runtimeUnavailable
}

struct RuntimeFundingOpener: FundingOpening {
    let runtimeProvider: TrUAPIHostRuntimeProviding?

    func openFunding(direction: FundingDirection) async throws -> String? {
        guard let runtimeProvider else { throw FundingOpeningError.runtimeUnavailable }

        return try await runtimeProvider.sharedRuntime().openFunding(direction: direction)
    }
}
