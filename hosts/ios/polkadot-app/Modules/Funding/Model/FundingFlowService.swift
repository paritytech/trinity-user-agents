import Foundation
import TrUAPIHost

/// What a funding overlay asks of the core and of the providers. The calls
/// run off the main actor; the overlay's model publishes what they return.
protocol FundingFlowServiceProtocol: Sendable {
    var branding: FundingProviderBranding { get }

    /// What the core holds for session `intent`: what its overlay offers,
    /// the latest quote list, the session, and the session's progress.
    func options(intent: String) -> FundingOptions?
    func quotes(intent: String) -> FundingQuoteList?
    func session(intent: String) -> FundingSession?
    func progress(intent: String) -> FundingProgress?
    /// Why `amount` cannot go ahead for the reported selection, given what a
    /// withdrawal can spend.
    func amountIssue(intent: String, amount: U128, spendable: U128?) -> FundingAmountIssue?

    /// Tells the core what the user picked, and returns what the core offers
    /// for it, which it answers inline.
    func select(_ selection: FundingSelection, intent: String) async -> FundingOptions?
    /// Asks every provider routing the reported selection to quote `amount`,
    /// and returns the core's id for the request: the running one when it
    /// already prices that, nil while the selection names no asset or the
    /// amount is zero.
    func requestQuote(amount: U128, intent: String) async -> UInt64?
    /// Hands the session to request `requestId`'s best quote once every
    /// provider answered it.
    func startWhenQuoted(requestId: UInt64, intent: String) async
    func cancelAutoStart(intent: String) async
    func selectProvider(_ providerId: ProductId, quoteId: String, intent: String) async throws -> Bool
    func cancel(intent: String) async throws -> CancelOutcome
    func confirmCancel(intent: String, terms: CancelTerms?) async throws -> CancelConfirmation
    func brand(for providerId: ProductId) async -> FundingProviderBrand
}

/// Makes the overlay's calls on its own executor, so the core's synchronous
/// ones never hold up the main thread.
actor FundingFlowService: FundingFlowServiceProtocol {
    nonisolated let branding: FundingProviderBranding
    private nonisolated let runtime: FundingRuntime

    init(runtime: FundingRuntime, branding: FundingProviderBranding) {
        self.runtime = runtime
        self.branding = branding
    }

    nonisolated func options(intent: String) -> FundingOptions? {
        runtime.fundingOptions(intent: intent)
    }

    nonisolated func quotes(intent: String) -> FundingQuoteList? {
        runtime.fundingQuotes(intent: intent)
    }

    nonisolated func session(intent: String) -> FundingSession? {
        runtime.fundingSession(intent: intent)
    }

    nonisolated func progress(intent: String) -> FundingProgress? {
        runtime.fundingProgress(intent: intent)
    }

    nonisolated func amountIssue(intent: String, amount: U128, spendable: U128?) -> FundingAmountIssue? {
        runtime.fundingAmountIssue(intent: intent, amount: amount, spendable: spendable)
    }

    func select(_ selection: FundingSelection, intent: String) -> FundingOptions? {
        runtime.selectFundingOptions(intent: intent, selection: selection)
        return runtime.fundingOptions(intent: intent)
    }

    func requestQuote(amount: U128, intent: String) -> UInt64? {
        runtime.getFundingQuote(intent: intent, amount: amount)
    }

    func startWhenQuoted(requestId: UInt64, intent: String) {
        runtime.startFundingWhenQuoted(intent: intent, requestId: requestId)
    }

    func cancelAutoStart(intent: String) {
        runtime.cancelFundingAutoStart(intent: intent)
    }

    func selectProvider(_ providerId: ProductId, quoteId: String, intent: String) async throws -> Bool {
        try await runtime.selectFundingProvider(intent: intent, providerId: providerId, quoteId: quoteId)
    }

    func cancel(intent: String) async throws -> CancelOutcome {
        try await runtime.cancelFunding(intent: intent)
    }

    func confirmCancel(intent: String, terms: CancelTerms?) async throws -> CancelConfirmation {
        try await runtime.confirmFundingCancel(intent: intent, terms: terms)
    }

    func brand(for providerId: ProductId) async -> FundingProviderBrand {
        await branding.brand(for: providerId)
    }
}

/// Makes a session's core calls one after another, each off the main actor,
/// so the core sees the user's steps in the order they were taken.
@MainActor
final class FundingCoreCallQueue {
    private let service: FundingFlowServiceProtocol
    private var latest: Task<Void, Never>?

    init(service: FundingFlowServiceProtocol) {
        self.service = service
    }

    /// Makes `call` once the call before it has answered; `apply` takes its
    /// answer back on the main actor.
    func run<Answer: Sendable>(
        _ call: @escaping @Sendable (FundingFlowServiceProtocol) async -> Answer,
        then apply: @escaping @MainActor (Answer) -> Void = { _ in }
    ) {
        let previous = latest
        latest = Task { [service] in
            await previous?.value
            await apply(call(service))
        }
    }

    /// Returns once every call made so far, and every call their answers
    /// made, has answered.
    func idle() async {
        while let current = latest {
            await current.value
            if latest == current { return }
        }
    }
}
