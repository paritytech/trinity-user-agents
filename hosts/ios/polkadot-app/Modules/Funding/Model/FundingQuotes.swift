import Foundation
import Observation
import TrUAPIHost

/// The quotes one funding overlay shows: the core's latest list for the
/// request the overlay made last.
///
/// Requests go through the session's ``FundingCoreCallQueue``, after the
/// calls the overlay made before them, so the core prices the selection it
/// was told of last. A keypad change asks once the user pauses, timed on the
/// injected clock.
@MainActor
@Observable
final class FundingQuotes {
    private(set) var rows: [ProductId: FundingQuoteState] = [:]
    /// The providers of the core's latest quote list, in its order.
    private(set) var order: [ProductId] = []
    /// The provider the core marked as quoting best for the user.
    private(set) var bestProviderId: ProductId?
    /// When the core asks every provider again, and the prices on screen change.
    private(set) var refreshAt: Date?
    /// The core's latest list for the request, with what the providers'
    /// refusals say about it.
    private(set) var list: FundingQuoteList?
    /// Set from the moment a request is made until the core's first list for
    /// it is shown, or the core kept the running one.
    private(set) var isAwaiting = false

    /// Called after a list for the latest request is shown.
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

    /// Asks the core to price `amount` on the reported selection. The core
    /// keeps the running request when it already prices that, and its list
    /// then stands; otherwise none shows until the new request's list
    /// arrives. With `startWhenQuoted`, the core hands the session to the
    /// request's best quote once every provider answered it.
    func request(amount: U128?, startWhenQuoted: Bool = false) {
        guard let amount else {
            drop()
            return
        }

        isAwaiting = true
        let shownId = list?.requestId
        let askNumber = request.ask()
        coreCalls.run { [intent] service in
            let requestId = await service.requestQuote(amount: amount, intent: intent)
            if startWhenQuoted, let requestId {
                await service.startWhenQuoted(requestId: requestId, intent: intent)
            }
            return requestId
        } then: { [weak self] requestId in
            guard let self, request.isLatest(askNumber) else { return }
            guard let requestId else {
                drop()
                return
            }

            if let early = request.take(requestId, ofAsk: askNumber) {
                receive(early)
            } else if requestId == shownId {
                isAwaiting = false
                onShown?()
            } else {
                clear()
            }
        }
    }

    /// Shows no quote, and no list for any request made so far.
    func drop() {
        clear()
        isAwaiting = false
        request.drop()
    }

    /// The core withdrew the list: the user picked another route than it
    /// priced.
    func withdrawn() {
        guard !request.awaitsId else { return }

        clear()
    }

    /// Takes the core's quote list when it answers the latest request. The
    /// core keeps refreshing the last request after a drop, and a list can
    /// arrive before the core's id for its request does.
    func receive(_ quotes: FundingQuoteList) {
        guard request.shows(quotes) else { return }

        rows = Dictionary(quotes.rows.map { ($0.providerId, $0.state) }) { _, latest in latest }
        order = quotes.rows.map(\.providerId)
        bestProviderId = quotes.best
        refreshAt = Date(milliseconds: quotes.refreshAtMs)
        list = quotes
        isAwaiting = false
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

    private func clear() {
        rows = [:]
        order = []
        bestProviderId = nil
        refreshAt = nil
        list = nil
    }
}
