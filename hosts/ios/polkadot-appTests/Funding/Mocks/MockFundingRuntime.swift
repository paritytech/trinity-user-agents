import Foundation
import os
import TrUAPIHost
@testable import polkadot_app

/// A runtime answering each selection with fixed options, as the core does
/// inline, and recording what the funding screens asked of it. Like the
/// core, it offers the chosen token on crypto and the payment country's
/// currency on card and bank, prices the reported selection, and keeps the
/// running request for an unchanged ask.
final class MockFundingRuntime: FundingRuntime, Sendable {
    private struct State: Sendable {
        var options: FundingOptions?
        var sessions: [String: (session: FundingSession, progress: FundingProgress?)] = [:]
        var history: [FundingSessionSummary] = []
        var selections: [FundingSelection] = []
        var asks: [FundingQuoteAsk] = []
        var amountIssue: FundingAmountIssue?
        var autoStarts: [AutoStart] = []
        var selectedProviders: [ProductId] = []
        var acknowledged: [String] = []
        var holdsSelection = false
        var heldSelection: CheckedContinuation<Void, Never>?
        var quoteList: FundingQuoteList?
        var waitingFrames: Set<String> = []
        var openedFrames: [String] = []
    }

    /// What the screens asked of the core's start when quoted.
    enum AutoStart: Equatable {
        case armed(requestId: UInt64)
        case cancelled
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    init(
        candidates: [FundingCandidate]? = nil,
        rails: [FundingRail] = [.crypto, .card, .bank],
        detectedCountry: String? = nil
    ) {
        let options = candidates.map { candidates in
            FundingOptions(
                candidates: candidates,
                countries: [],
                limits: FundingAmountLimits(min: "10000000", max: nil),
                networks: [],
                rails: rails,
                asset: nil,
                tokens: ["USDC"],
                detectedCountry: detectedCountry
            )
        }
        state.withLock { $0.options = options }
    }

    var selections: [FundingSelection] { state.withLock { $0.selections } }
    var asks: [FundingQuoteAsk] { state.withLock { $0.asks } }
    var autoStarts: [AutoStart] { state.withLock { $0.autoStarts } }
    var selectedProviders: [ProductId] { state.withLock { $0.selectedProviders } }
    var acknowledged: [String] { state.withLock { $0.acknowledged } }
    var openedFrames: [String] { state.withLock { $0.openedFrames } }

    /// Holds a provider's screen for session `intent` until the app opens it,
    /// or drops it when `waiting` is false.
    func holdFrame(_ intent: String, waiting: Bool = true) {
        state.withLock { state in
            if waiting {
                state.waitingFrames.insert(intent)
            } else {
                state.waitingFrames.remove(intent)
            }
        }
    }

    /// The id of the latest quote request, which counts them from 1.
    var lastRequestId: UInt64 { UInt64(asks.count) }

    /// Answers every amount with `issue`.
    func answerAmounts(with issue: FundingAmountIssue?) {
        state.withLock { $0.amountIssue = issue }
    }

    /// Keeps the next provider selection waiting until ``releaseSelection()``.
    func holdSelection() {
        state.withLock { $0.holdsSelection = true }
    }

    func releaseSelection() {
        let held = state.withLock { state in
            state.holdsSelection = false
            defer { state.heldSelection = nil }
            return state.heldSelection
        }
        held?.resume()
    }

    /// Holds `session` as the core would, with `progress`.
    func hold(_ session: FundingSession, progress: FundingProgress? = nil) {
        state.withLock { $0.sessions[session.intent] = (session, progress) }
    }

    /// Holds `list` as the core's latest quote list, or none.
    func list(_ list: FundingQuoteList?) {
        state.withLock { $0.quoteList = list }
    }

    /// Lists `summaries` as the core's history.
    func record(history summaries: [FundingSessionSummary]) {
        state.withLock { $0.history = summaries }
    }

    func fundingSession(intent: String) -> FundingSession? { state.withLock { $0.sessions[intent]?.session } }
    func fundingProgress(intent: String) -> FundingProgress? { state.withLock { $0.sessions[intent]?.progress } }
    func fundingOptions(intent _: String) -> FundingOptions? { state.withLock { $0.options } }
    func fundingQuotes(intent _: String) -> FundingQuoteList? { state.withLock { $0.quoteList } }
    func fundingEntries() -> [FundingBookEntry] {
        state.withLock { state in
            state.sessions.values.compactMap { held in
                held.progress.map { FundingBookEntry(session: held.session, progress: $0) }
            }
        }
    }

    func selectFundingOptions(intent _: String, selection: FundingSelection) {
        state.withLock { state in
            state.selections.append(selection)
            state.options?.asset = selection.rail == .crypto ? selection.asset : selection.currency ?? "EUR"
        }
    }

    func fundingAmountIssue(intent _: String, amount _: U128, spendable _: U128?) -> FundingAmountIssue? {
        state.withLock { $0.amountIssue }
    }

    func getFundingQuote(intent _: String, amount: U128) -> UInt64? {
        state.withLock { state in
            guard amount != "0", let selection = state.selections.last, let asset = state.options?.asset else {
                return nil
            }

            let ask = FundingQuoteAsk(
                direction: .in,
                rail: selection.rail,
                asset: asset,
                network: selection.network,
                amount: amount,
                country: selection.country
            )
            if ask != state.asks.last { state.asks.append(ask) }
            return UInt64(state.asks.count)
        }
    }

    func startFundingWhenQuoted(intent _: String, requestId: UInt64) {
        state.withLock { $0.autoStarts.append(.armed(requestId: requestId)) }
    }

    func cancelFundingAutoStart(intent _: String) {
        state.withLock { $0.autoStarts.append(.cancelled) }
    }

    func selectFundingProvider(intent _: String, providerId: ProductId, quoteId _: String) async throws -> Bool {
        await withCheckedContinuation { continuation in
            let holds = state.withLock { state in
                state.selectedProviders.append(providerId)
                if state.holdsSelection { state.heldSelection = continuation }
                return state.holdsSelection
            }
            if !holds { continuation.resume() }
        }
        return true
    }

    func cancelFunding(intent _: String) async throws -> CancelOutcome { .ended }
    func confirmFundingCancel(intent _: String, terms _: CancelTerms?) async throws -> CancelConfirmation { .declined }
    func fundingHistory() -> [FundingSessionSummary] { state.withLock { $0.history } }
    func fundingSummary(intent: String) -> FundingSessionSummary? {
        state.withLock { $0.history.first { $0.intent == intent } }
    }

    func fundingSessionOpen(intent: String) -> Bool {
        state.withLock { $0.sessions[intent].map(\.session.stage.isOpen) ?? false }
    }

    func openFunding(direction _: FundingDirection, amount _: U128?) async throws -> String? { nil }

    func openProviderFrame(intent: String) -> Bool {
        state.withLock { state in
            state.openedFrames.append(intent)
            return state.waitingFrames.remove(intent) != nil
        }
    }

    func fundingFrameWaiting(intent: String) -> Bool {
        state.withLock { $0.waitingFrames.contains(intent) }
    }

    func acknowledgeFundingSession(intent: String) async throws -> Bool {
        state.withLock { $0.acknowledged.append(intent) }
        return true
    }
}

/// Branding that names every provider after its id, counting the reads.
final class MockFundingBranding: FundingProviderBranding {
    private let reads = OSAllocatedUnfairLock(initialState: [ProductId]())

    var readProviders: [ProductId] { reads.withLock { $0 } }

    func brand(for providerId: ProductId) async -> FundingProviderBrand {
        reads.withLock { $0.append(providerId) }
        return FundingProviderBrand(name: providerId.rawValue, icon: nil)
    }
}
