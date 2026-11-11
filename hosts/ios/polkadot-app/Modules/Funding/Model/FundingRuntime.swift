import Foundation
import TrUAPIHost

/// The part of the runtime the funding screens drive. The core owns every
/// session and keeps what the screens draw: it says when something changed,
/// and the screens read it back here, report what the user picked, price the
/// session and hand it to a provider.
protocol FundingRuntime: AnyObject, Sendable {
    func fundingSession(intent: String) -> FundingSession?
    func fundingProgress(intent: String) -> FundingProgress?
    func fundingOptions(intent: String) -> FundingOptions?
    func fundingQuotes(intent: String) -> FundingQuoteList?
    func fundingEntries() -> [FundingBookEntry]
    func selectFundingOptions(intent: String, selection: FundingSelection)
    func fundingAmountIssue(intent: String, amount: U128, spendable: U128?) -> FundingAmountIssue?
    @discardableResult
    func getFundingQuote(intent: String, amount: U128) -> UInt64?
    func startFundingWhenQuoted(intent: String, requestId: UInt64)
    func cancelFundingAutoStart(intent: String)
    func selectFundingProvider(intent: String, providerId: ProductId, quoteId: String) async throws -> Bool
    func cancelFunding(intent: String) async throws -> CancelOutcome
    func confirmFundingCancel(intent: String, terms: CancelTerms?) async throws -> CancelConfirmation
    func fundingHistory() -> [FundingSessionSummary]
    @discardableResult
    func acknowledgeFundingSession(intent: String) async throws -> Bool
    func openFunding(direction: FundingDirection, amount: U128?) async throws -> String?
}

extension TrUAPIHostRuntime: FundingRuntime {}

extension FundingQuoteState {
    var quote: FundingQuote? {
        guard case let .quoted(quote) = self else { return nil }
        return quote
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
