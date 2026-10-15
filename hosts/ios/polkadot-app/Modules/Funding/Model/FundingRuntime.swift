import Foundation
import TrUAPIHost

/// The part of the runtime the funding screens drive. The core owns every
/// session: the screens read it, price it and hand it to a provider.
protocol FundingRuntime: AnyObject, Sendable {
    func fundingSession(intent: String) -> FundingSession?
    func fundingProgress(intent: String) -> FundingProgress?
    func fundingSessions() -> [FundingSession]
    func fundingInProgress() async throws -> Bool
    func fundingCandidates(intent: String) -> [FundingCandidate]
    @discardableResult
    func getFundingQuote(intent: String, ask: FundingQuoteAsk) -> UInt64
    func stopFundingQuotes(intent: String)
    func fundingCountries(intent: String, rail: FundingRail, asset: String) -> [FundingCountry]
    func fundingAmountLimits(
        intent: String,
        rail: FundingRail,
        asset: String,
        network: String?,
        country: String?
    ) -> FundingAmountLimits
    func detectedFundingCountry() async -> String?
    func selectFundingProvider(intent: String, providerId: String, quoteId: String?) async throws -> Bool
    func cancelFunding(intent: String) async throws -> CancelOutcome
    func confirmFundingCancel(intent: String, accept: Bool) async throws -> CancelConfirmation
    func acknowledgeFundingSession(intent: String) async throws -> Bool
    func openFunding(direction: FundingDirection, amount: U128?) async throws -> String?
}

extension TrUAPIHostRuntime: FundingRuntime {}

extension FundingRail {
    static let displayOrder: [FundingRail] = [.crypto, .card, .bank]
}

extension FundingCandidate {
    func routes(rail: FundingRail, direction: FundingDirection) -> [FundingRoute] {
        routes.filter { $0.rail == rail && $0.directions.contains(direction) }
    }

    func serves(rail: FundingRail, direction: FundingDirection) -> Bool {
        !routes(rail: rail, direction: direction).isEmpty
    }
}

extension FundingQuoteState {
    var quote: FundingQuote? {
        guard case let .quoted(quote) = self else { return nil }
        return quote
    }

    var refusal: FundingQuoteRefusal? {
        guard case let .unavailable(.refused(reason)) = self else { return nil }
        return reason
    }

    var isPending: Bool {
        if case .pending = self { return true }
        return false
    }
}

extension FundingStage {
    var isOpen: Bool {
        if case .open = self { return true }
        return false
    }
}
