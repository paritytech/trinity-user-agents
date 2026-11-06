import Foundation
import Observation
import TrUAPIHost

/// The quotes one funding overlay shows: the ask the providers were sent
/// last, and the core's latest list for it.
///
/// Asks go through the session's ``FundingCoreCallQueue``, after the calls
/// the overlay made before them. A keypad change asks once the user pauses,
/// timed on the injected clock.
@MainActor
@Observable
final class FundingQuotes {
    /// What every provider was asked last.
    private(set) var ask: FundingQuoteAsk?
    private(set) var rows: [ProductId: FundingQuoteState] = [:]
    /// The providers of the core's latest quote list, in its order.
    private(set) var order: [ProductId] = []
    /// The provider the core marked as quoting best for the user.
    private(set) var bestProviderId: ProductId?
    /// When the core asks every provider again, and the prices on screen change.
    private(set) var refreshAt: Date?

    /// Called after a list for the latest ask is shown.
    @ObservationIgnored var onShown: (() -> Void)?

    private let intent: String
    private let coreCalls: FundingCoreCallQueue
    private let clock: any Clock<Duration>
    @ObservationIgnored private var request = FundingQuoteRequest()
    /// The ask waiting on the keypad pause.
    @ObservationIgnored private var pause: Task<Void, Never>?

    init(intent: String, coreCalls: FundingCoreCallQueue, clock: any Clock<Duration>) {
        self.intent = intent
        self.coreCalls = coreCalls
        self.clock = clock
    }

    /// Asks every provider in `providerIds` for `newAsk`, and shows them all
    /// pending until the core's list for it arrives.
    func request(_ newAsk: FundingQuoteAsk, from providerIds: [ProductId]) {
        ask = newAsk
        refreshAt = nil
        bestProviderId = nil
        rows = Dictionary(uniqueKeysWithValues: providerIds.map { ($0, .pending) })
        order = []
        let askNumber = request.ask()
        coreCalls.run { [intent] service in
            await service.requestQuote(newAsk, intent: intent)
        } then: { [weak self] requestId in
            guard let self, let early = request.take(requestId, ofAsk: askNumber) else { return }

            receive(early)
        }
    }

    /// Shows no quote, and no list for any ask made so far.
    func drop() {
        rows = [:]
        order = []
        bestProviderId = nil
        ask = nil
        request.drop()
    }

    /// Takes the core's quote list when it answers the latest ask. The core
    /// keeps refreshing the last ask after a drop, and a list can arrive
    /// before the core's id for its ask does.
    func receive(_ quotes: FundingQuoteList) {
        guard request.shows(quotes) else { return }

        rows = Dictionary(quotes.rows.map { ($0.providerId, $0.state) }) { _, latest in latest }
        order = quotes.rows.map(\.providerId)
        bestProviderId = quotes.best
        refreshAt = Date(milliseconds: quotes.refreshAtMs)
        onShown?()
    }

    /// Calls `ask` once the keypad has paused, replacing the call waiting
    /// on the pause before it.
    func askAfterPause(_ ask: @escaping @MainActor () -> Void) {
        pause?.cancel()
        pause = Task { [clock] in
            try? await clock.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }

            ask()
        }
    }

    /// Drops the call waiting on the keypad pause.
    func cancelPause() {
        pause?.cancel()
    }

    /// Returns once the call waiting on the keypad pause has finished.
    func pauseSettled() async {
        await pause?.value
    }
}
