import Foundation
import TrUAPIHost

/// One funding session as the screens draw it: what the core holds for it,
/// and the quotes for the latest ask made on it.
struct FundingEntry: Equatable, Sendable {
    enum Phase: Equatable, Sendable {
        /// Not handed to a provider yet: what the core offers for the
        /// reported selection.
        case selecting(FundingOptions?)
        /// Handed to a provider and in flight.
        case running
        /// Over.
        case ended
    }

    /// The session's intent.
    let id: String
    let direction: FundingDirection
    let amount: U128?
    let openedAtMs: UInt64
    let session: FundingSession?
    let progress: FundingProgress?
    /// How an ended session went, from the core's history.
    let summary: FundingSessionSummary?
    /// The core's list for the latest ask; nil while none answers it.
    let quotes: FundingQuoteList?
    /// An ask waits for its first list.
    let isQuoting: Bool
    let phase: Phase
}

extension FundingEntry {
    init(
        id: String,
        direction: FundingDirection,
        amount: U128?,
        session: FundingSession?,
        progress: FundingProgress?,
        summary: FundingSessionSummary?,
        options: FundingOptions?,
        quotes: FundingQuoteList?,
        isQuoting: Bool
    ) {
        self.id = id
        self.direction = session?.direction ?? summary?.direction ?? direction
        self.amount = session?.amount ?? summary?.amount ?? amount
        openedAtMs = session?.openedAtMs ?? summary?.openedAtMs ?? 0
        self.session = session
        self.progress = progress
        self.summary = summary
        self.quotes = quotes
        self.isQuoting = isQuoting
        phase = Self.phase(session: session, summary: summary, options: options)
    }

    /// The core's offer for the reported selection, while the session is
    /// being set up.
    var options: FundingOptions? {
        guard case let .selecting(options) = phase else { return nil }
        return options
    }

    var hasStarted: Bool {
        phase != .selecting(options)
    }

    var hasEnded: Bool {
        phase == .ended
    }
}

// MARK: - Session

extension FundingEntry {
    /// Why the session ended, once it has failed.
    var failure: FundingFailure? {
        guard case let .failed(reason, _) = session?.stage else { return nil }
        return reason
    }

    /// The session once it has ended, as its ended screen draws it.
    var ended: FundingEndedSession? {
        session.flatMap { FundingEndedSession(session: $0, progress: progress) }
    }

    /// Whether the session got to `step`.
    func hasReached(_ step: FundingStep) -> Bool {
        progress?.steps.contains { $0.step == step && $0.reachedAtMs != nil } ?? false
    }

    /// A bank or crypto top-up whose payment the provider has not seen:
    /// cancelling ends it outright, and money sent anyway still arrives.
    func isUnpaidTopUp(rail: FundingRail) -> Bool {
        direction == .in && (session?.choice?.rail ?? rail) != .card && !hasReached(.received)
    }

    /// The provider's terms for cancelling, while it waits for the user.
    var cancelOffer: (fee: U128, refund: U128)? {
        guard case let .cancelOffered(fee, refund) = progress?.status else { return nil }
        return (fee, refund)
    }

    /// Whether the user can cancel: the session is in flight, and either no
    /// cancel is with the provider or no money moved yet, when a second cancel
    /// ends the session. The core says when it is too late.
    var canRequestCancel: Bool {
        guard hasStarted, let progress, progress.status.isInFlight else { return false }
        return progress.status.canCancel(moneyMoved: progress.moneyMoved)
    }

    /// The cancel action's title for the session's status.
    var cancelTitle: String {
        progress.map(\.status.cancelTitle) ?? String(localized: .Funding.depositCancel)
    }
}

private extension FundingEntry {
    static func phase(
        session: FundingSession?,
        summary: FundingSessionSummary?,
        options: FundingOptions?
    ) -> Phase {
        guard let session else {
            return summary == nil ? .selecting(options) : .ended
        }
        guard session.stage.isOpen else { return .ended }
        return session.providerId == nil ? .selecting(options) : .running
    }
}
