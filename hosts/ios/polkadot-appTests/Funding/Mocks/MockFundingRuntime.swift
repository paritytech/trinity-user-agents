import Foundation
import os
import TrUAPIHost
@testable import polkadot_app

/// A runtime answering each selection with the same options, as the core does
/// inline, and recording what the funding screens asked of it.
final class MockFundingRuntime: FundingRuntime, Sendable {
    struct Refused: Error {}

    private struct State: Sendable {
        var selections: [FundingSelection] = []
        var asks: [FundingQuoteAsk] = []
        var selectedProviders: [ProductId] = []
        var acknowledged: [String] = []
        var failsNextAcknowledgement = false
        var holdsSelection = false
        var heldSelection: CheckedContinuation<Void, Never>?
    }

    let book = FundingSessionBook(logger: MockLogger())
    private let options: FundingOptions?
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(candidates: [FundingCandidate]? = nil, failsFirstAcknowledgement: Bool = false) {
        options = candidates.map { candidates in
            FundingOptions(
                candidates: candidates,
                countries: [],
                limits: FundingAmountLimits(min: "10000000", max: nil),
                networks: []
            )
        }
        state.withLock { $0.failsNextAcknowledgement = failsFirstAcknowledgement }
        if let options { book.record(intent: "intent", options: options) }
    }

    var selections: [FundingSelection] { state.withLock { $0.selections } }
    var asks: [FundingQuoteAsk] { state.withLock { $0.asks } }
    var selectedProviders: [ProductId] { state.withLock { $0.selectedProviders } }
    var acknowledged: [String] { state.withLock { $0.acknowledged } }
    /// The id of the latest quote request, which counts them from 1.
    var lastRequestId: UInt64 { UInt64(asks.count) }

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

    func selectFundingOptions(intent: String, selection: FundingSelection) {
        state.withLock { $0.selections.append(selection) }
        if let options { book.record(intent: intent, options: options) }
    }

    func getFundingQuote(intent _: String, ask: FundingQuoteAsk) -> UInt64 {
        state.withLock { state in
            state.asks.append(ask)
            return UInt64(state.asks.count)
        }
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
    func fundingHistory() -> [FundingSessionSummary] { [] }
    func openFunding(direction _: FundingDirection, amount _: U128?) async throws -> String? { nil }

    func acknowledgeFundingSession(intent: String) async throws -> Bool {
        let fails = state.withLock { state in
            state.acknowledged.append(intent)
            defer { state.failsNextAcknowledgement = false }
            return state.failsNextAcknowledgement
        }
        if fails { throw Refused() }
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
