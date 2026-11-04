import Foundation
import TrUAPIHost
import UIKitExt

/// Opens a funding session on the host's own behalf, as the CASH card does.
/// Answers the session id, or `nil` when the user dismissed the overlay or
/// was shown the sessions still in flight instead.
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

/// How opening the overlay went, for the screen it was opened from.
enum FundingOpened {
    /// A session started.
    case started
    /// The user closed the overlay, or was shown the sessions in flight.
    case notStarted
    /// The opening task was cancelled, as when the user left the screen;
    /// there is nothing to tell them.
    case cancelled
    /// It could not open, which the user is told.
    case failed(Error)

    init(_ result: Result<String?, Error>) {
        switch result {
        case .success(.some): self = .started
        case .success(.none): self = .notStarted
        case let .failure(error): self = error is CancellationError ? .cancelled : .failed(error)
        }
    }
}

struct RuntimeFundingOpener: FundingOpening {
    let runtimeProvider: TrUAPIHostRuntimeProviding?
    /// Shows the sessions in flight in a direction, answering false when
    /// there are none.
    var showPending: @Sendable (FundingDirection) async -> Bool = { direction in
        await FundingActivityCenter.shared.showPending(direction: direction)
    }

    func openFunding(direction: FundingDirection) async throws -> String? {
        guard let runtimeProvider else {
            Logger.shared.error("[funding] opening the overlay failed: no runtime provider")
            throw FundingOpeningError.runtimeUnavailable
        }
        if await showPending(direction) { return nil }

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
