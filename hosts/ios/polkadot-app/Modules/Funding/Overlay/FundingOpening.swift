import Foundation
import TrUAPIHost
import UIKitExt

/// Opens a funding session on the host's own behalf, as the CASH card does.
/// Answers the session id, or `nil` when the user dismissed the overlay.
protocol FundingOpening: Sendable {
    func openFunding(direction: FundingDirection) async throws -> String?
}

/// The overlay could not be opened. Says so in the user's words, since what
/// the core answered is not meant for them.
enum FundingOpeningError: Error, ErrorContentConvertible {
    case runtimeUnavailable
    case refused

    func toErrorContent() -> ErrorContent {
        ErrorContent(
            title: String(localized: .Funding.errorOpenTitle),
            message: String(localized: .Funding.errorOpenMessage)
        )
    }
}

struct RuntimeFundingOpener: FundingOpening {
    let runtimeProvider: TrUAPIHostRuntimeProviding?

    func openFunding(direction: FundingDirection) async throws -> String? {
        guard let runtimeProvider else { throw FundingOpeningError.runtimeUnavailable }

        do {
            return try await runtimeProvider.sharedRuntime().openFunding(direction: direction)
        } catch let error as CancellationError {
            throw error
        } catch {
            Logger.shared.error("[funding] opening the overlay failed: \(error)")
            throw FundingOpeningError.refused
        }
    }
}
