import AsyncExtensions
import Foundation
import os
import SubstrateSdk
import TrUAPIHost

/// The one place the funding screens reach the core. Reads what the core
/// holds when it says something changed, and pushes it to whoever follows a
/// session or the list of sessions.
///
/// Calls into the core that change it run on one serial queue, off the
/// caller's thread, in the order they were made: the core is synchronous,
/// and must see the user's steps in the order they were taken.
///
/// Built before the runtime, which is built off the main thread and hands
/// itself over with ``attach(runtime:)``. Until then the service reads no
/// sessions, and a call that needs the core waits for it.
final class FundingService: FundingServiceProtocol, @unchecked Sendable {
    let events: AnyAsyncSequence<FundingEvent>

    private let logger: LoggerProtocol
    private let eventSink: AsyncStream<FundingEvent>.Continuation
    private let state = OSAllocatedUnfairLock(initialState: State())
    private let runtimeSlot = FundingRuntimeSlot()
    private let calls = DispatchQueue(label: "io.parity.funding.core-calls")

    init(logger: LoggerProtocol) {
        self.logger = logger
        let (stream, sink) = AsyncStream<FundingEvent>.makeStream()
        events = stream.eraseToAnyAsyncSequence()
        eventSink = sink
    }

    /// Hands the service the runtime it was installed on, once it is built,
    /// and shows the sessions it holds to those following the list.
    func attach(runtime: FundingRuntime) {
        runtimeSlot.fill(runtime)
        publishEntries()
    }

    func subscribeEntries() -> AnyAsyncSequence<[FundingEntry]> {
        let (stream, continuation) = AsyncStream<[FundingEntry]>.makeStream(bufferingPolicy: .bufferingNewest(1))
        let id = UUID()
        state.withLock { $0.listFollowers[id] = continuation }
        continuation.onTermination = { [weak self] _ in
            self?.state.withLock { _ = $0.listFollowers.removeValue(forKey: id) }
        }
        continuation.yield(entries())
        return stream.eraseToAnyAsyncSequence()
    }

    func subscribeEntryState(_ entryId: String) -> AnyAsyncSequence<FundingEntry> {
        let (stream, continuation) = AsyncStream<FundingEntry>.makeStream(bufferingPolicy: .bufferingNewest(1))
        let id = UUID()
        state.withLock { $0.entryFollowers[entryId, default: [:]][id] = continuation }
        continuation.onTermination = { [weak self] _ in
            self?.state.withLock { _ = $0.entryFollowers[entryId]?.removeValue(forKey: id) }
        }
        let current = entry(entryId)
        continuation.yield(current)
        if current.hasEnded { continuation.finish() }
        return stream.eraseToAnyAsyncSequence()
    }

    func open(_ direction: FundingDirection, amount: Balance?) async throws -> String? {
        try await runtimeSlot.built().openFunding(direction: direction, amount: amount?.units)
    }

    func select(_ selection: FundingSelection, for entryId: String) {
        calls.async { [weak self] in
            self?.runtime?.selectFundingOptions(intent: entryId, selection: selection)
            self?.publish(entryId)
        }
    }

    func quote(amount: Balance?, for entryId: String) {
        guard let amount else {
            state.withLock { $0.quotes[entryId, default: FundingSessionQuotes()].drop() }
            publish(entryId)
            return
        }

        let ask = state.withLock { $0.quotes[entryId, default: FundingSessionQuotes()].ask() }
        publish(entryId)
        calls.async { [weak self] in
            let requestId = self?.runtime?.getFundingQuote(intent: entryId, amount: amount.units)
            self?.take(requestId, of: ask, for: entryId)
        }
    }

    func amountIssue(_ amount: Balance, spendable: Balance?, for entryId: String) -> FundingAmountIssue? {
        runtime?.fundingAmountIssue(intent: entryId, amount: amount.units, spendable: spendable?.units)
    }

    func start(_ entryId: String, provider: ProductId?) async throws {
        guard let provider else {
            state.withLock { $0.quotes[entryId, default: FundingSessionQuotes()].startsWhenQuoted = true }
            await withCheckedContinuation { continuation in
                calls.async { [weak self] in
                    self?.startWhenQuoted(entryId)
                    continuation.resume()
                }
            }
            return
        }
        guard let quoteId = shownQuote(of: provider, for: entryId)?.quoteId else {
            throw FundingStartError.noQuote
        }

        let selected = try await runtimeSlot.built()
            .selectFundingProvider(intent: entryId, providerId: provider, quoteId: quoteId)
        guard isPresenting(entryId) else { throw FundingStartError.left }
        guard selected else {
            publish(entryId)
            throw FundingStartError.notSelected(entry(entryId).failure)
        }

        resolvePresentation(entryId, with: .started)
        publish(entryId)
    }

    func stopStarting(_ entryId: String) {
        let wasStarting = state.withLock { state in
            defer {
                state.quotes[entryId]?.startsWhenQuoted = false
                state.quotes[entryId]?.startedOn = nil
            }
            return state.quotes[entryId]?.startsWhenQuoted == true
        }
        guard wasStarting else { return }

        calls.async { [weak self] in self?.runtime?.cancelFundingAutoStart(intent: entryId) }
    }

    func leave(_ entryId: String) {
        guard isPresenting(entryId) else { return }

        stopStarting(entryId)
        resolvePresentation(entryId, with: .dismissed)
        finishFollowers(of: entryId)
    }

    func cancel(_ entryId: String) async throws -> CancelOutcome {
        guard !isPresenting(entryId) else {
            leave(entryId)
            return .cancelled
        }

        let outcome = try await runtimeSlot.built().cancelFunding(intent: entryId)
        publish(entryId)
        return outcome
    }

    func answerCancelOffer(_ entryId: String, accepting terms: CancelTerms?) async throws -> CancelConfirmation {
        let answer = try await runtimeSlot.built().confirmFundingCancel(intent: entryId, terms: terms)
        publish(entryId)
        return answer
    }

    func acknowledge(_ entryId: String) async {
        do {
            try await runtimeSlot.built().acknowledgeFundingSession(intent: entryId)
        } catch {
            logger.error("[funding] \(entryId): acknowledging the ended session failed: \(error)")
        }
        publishEntries()
    }

    func openProviderFrame(_ request: FundingFrameRequest) {
        state.withLock { _ = $0.requestedFrames.remove(request.entryId) }
        calls.async { [weak self, logger] in
            guard self?.runtime?.openProviderFrame(intent: request.entryId) != true else { return }

            logger.debug("[funding] \(request.entryId): \(request.providerId) no longer waits to show a screen")
        }
    }

    func waitingFrames() async -> [FundingFrameRequest] {
        guard let runtime = try? await runtimeSlot.built() else { return [] }

        let waiting = runtime.fundingEntries().compactMap { item -> FundingFrameRequest? in
            guard let providerId = item.session.providerId,
                  runtime.fundingFrameWaiting(intent: item.session.intent)
            else { return nil }

            return FundingFrameRequest(entryId: item.session.intent, providerId: providerId)
        }
        state.withLock { $0.requestedFrames.formUnion(waiting.map(\.entryId)) }
        return waiting
    }

    func answer(frame: FundingFrame.ID, with outcome: FundingFrameOutcome) {
        let waiting = state.withLock { $0.frames.removeValue(forKey: frame) }
        waiting?.answer.resume(returning: outcome)
    }
}

// MARK: - The core's side

extension FundingService: FundingCoreEventHandling {
    func presentFunding(intent: String, direction: FundingDirection, amount: U128?) async -> FundingPresentOutcome {
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                state.withLock { state in
                    state.presentations[intent] = Presentation(
                        direction: direction,
                        amount: Balance(units: amount),
                        answer: continuation
                    )
                }
                eventSink.yield(.presentationRequested(entry(intent)))
            }
        } onCancel: {
            resolvePresentation(intent, with: .dismissed)
            finishFollowers(of: intent)
        }
    }

    func presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget
    ) async -> FundingFrameOutcome {
        let frame = FundingFrame(
            id: UUID(),
            entryId: intent,
            providerId: providerId,
            target: target,
            awaitsUserTransfer: runtime?.fundingProgress(intent: intent)?.awaitsUserTransfer ?? false
        )
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                state.withLock { $0.frames[frame.id] = PendingFrame(entryId: intent, answer: continuation) }
                eventSink.yield(.presentFrame(frame))
            }
        } onCancel: {
            answer(frame: frame.id, with: .dismissed)
            eventSink.yield(.frameClosed(entryId: intent))
        }
    }

    /// The provider is done with its screens for session `intent`: each one
    /// still open answers `.closed`.
    func closeProviderFrame(providerId _: ProductId, intent: String) {
        let closed = state.withLock { state in
            let ids = state.frames.filter { $0.value.entryId == intent }.map(\.key)
            return ids.compactMap { state.frames.removeValue(forKey: $0) }
        }
        for frame in closed {
            frame.answer.resume(returning: .closed)
        }
        eventSink.yield(.frameClosed(entryId: intent))
    }

    /// A provider asks to show a screen; the app opens it when the user is
    /// looking at the session, or after a tap on a notice.
    func providerFrameRequested(intent: String, providerId: ProductId) {
        state.withLock { _ = $0.requestedFrames.insert(intent) }
        eventSink.yield(.frameRequested(FundingFrameRequest(entryId: intent, providerId: providerId)))
    }

    func fundingChanged(intent: String, kind: FundingChangeKind) {
        switch kind {
        case .session:
            withdrawFrameRequest(intent)
            startedWhenQuoted(intent)
            publish(intent)
            publishEntries()
        case .options:
            publish(intent)
        case .quotes:
            takeQuotes(for: intent)
        case .providers:
            break
        }
    }

    func workerCrashing(productId: ProductId) {
        eventSink.yield(.providerNotWorking(productId))
    }
}

// MARK: - Private

private extension FundingService {
    struct PendingFrame {
        let entryId: String
        let answer: CheckedContinuation<FundingFrameOutcome, Never>
    }

    struct Presentation {
        let direction: FundingDirection
        let amount: Balance?
        let answer: CheckedContinuation<FundingPresentOutcome, Never>
    }

    struct State {
        var presentations: [String: Presentation] = [:]
        var frames: [FundingFrame.ID: PendingFrame] = [:]
        /// Sessions whose provider asked for a screen the app has not opened.
        var requestedFrames: Set<String> = []
        var quotes: [String: FundingSessionQuotes] = [:]
        var entryFollowers: [String: [UUID: AsyncStream<FundingEntry>.Continuation]] = [:]
        var listFollowers: [UUID: AsyncStream<[FundingEntry]>.Continuation] = [:]
    }

    var runtime: FundingRuntime? {
        runtimeSlot.runtime
    }

    /// The core dropped the screen session `entryId`'s provider asked for, as
    /// its payment moved or it ended: the notice about it goes.
    func withdrawFrameRequest(_ entryId: String) {
        let requested = state.withLock { $0.requestedFrames.contains(entryId) }
        guard requested, let runtime, !runtime.fundingFrameWaiting(intent: entryId) else { return }

        state.withLock { _ = $0.requestedFrames.remove(entryId) }
        eventSink.yield(.frameClosed(entryId: entryId))
    }

    func isPresenting(_ entryId: String) -> Bool {
        state.withLock { $0.presentations[entryId] != nil }
    }

    func resolvePresentation(_ entryId: String, with outcome: FundingPresentOutcome) {
        let presentation = state.withLock { $0.presentations.removeValue(forKey: entryId) }
        presentation?.answer.resume(returning: outcome)
    }

    func shownQuote(of provider: ProductId, for entryId: String) -> FundingQuote? {
        state.withLock { $0.quotes[entryId]?.shown }?
            .rows.first { $0.providerId == provider }?.state.quote
    }

    /// Takes the core's id for ask `ask`, unless a later ask or a drop
    /// replaced it, and shows the list that came before the id.
    func take(_ requestId: UInt64?, of ask: (number: Int, shownId: UInt64?), for entryId: String) {
        let startsNow = state.withLock { state -> Bool in
            var quotes = state.quotes[entryId, default: FundingSessionQuotes()]
            defer { state.quotes[entryId] = quotes }
            guard quotes.request.isLatest(ask.number) else { return false }
            guard let requestId else {
                quotes.drop()
                return false
            }

            quotes.requestId = requestId
            if let early = quotes.request.take(requestId, ofAsk: ask.number) {
                quotes.shown = early
                quotes.isAwaiting = false
            } else if requestId == ask.shownId {
                quotes.isAwaiting = false
            } else {
                quotes.shown = nil
            }
            return quotes.startsWhenQuoted
        }
        if startsNow { startWhenQuoted(entryId) }
        publish(entryId)
    }

    /// Asks the core to hand the session to the latest ask's best quote.
    /// Runs on the call queue, after the ask that returned the id.
    func startWhenQuoted(_ entryId: String) {
        let requestId = state.withLock { state -> UInt64? in
            guard let quotes = state.quotes[entryId], quotes.startsWhenQuoted,
                  let requestId = quotes.requestId, requestId != quotes.startedOn
            else { return nil }

            state.quotes[entryId]?.startedOn = requestId
            return requestId
        }
        guard let requestId else { return }

        runtime?.startFundingWhenQuoted(intent: entryId, requestId: requestId)
    }

    /// A session the core handed to its best quote is reported started.
    func startedWhenQuoted(_ entryId: String) {
        let armed = state.withLock { $0.quotes[entryId]?.startsWhenQuoted == true }
        guard armed, isPresenting(entryId), runtime?.fundingSession(intent: entryId)?.providerId != nil else { return }

        state.withLock { $0.quotes[entryId]?.startsWhenQuoted = false }
        resolvePresentation(entryId, with: .started)
    }

    func takeQuotes(for entryId: String) {
        let list = runtime?.fundingQuotes(intent: entryId)
        state.withLock { state in
            guard var quotes = state.quotes[entryId] else { return }
            defer { state.quotes[entryId] = quotes }
            guard let list else {
                if !quotes.request.awaitsId { quotes.shown = nil }
                return
            }
            if quotes.request.shows(list) {
                quotes.shown = list
                quotes.isAwaiting = false
            }
        }
        publish(entryId)
    }

    func entry(_ entryId: String) -> FundingEntry {
        let runtime = runtime
        let session = runtime?.fundingSession(intent: entryId)
        let (presentation, quotes) = state.withLock { ($0.presentations[entryId], $0.quotes[entryId]) }
        return FundingEntry(
            id: entryId,
            direction: presentation?.direction ?? session?.direction ?? .in,
            amount: presentation?.amount,
            session: session,
            progress: runtime?.fundingProgress(intent: entryId),
            summary: session?.stage.isOpen == true ? nil : runtime?.fundingSummary(intent: entryId),
            options: session?.providerId == nil ? runtime?.fundingOptions(intent: entryId) : nil,
            quotes: quotes?.shown,
            isQuoting: quotes?.isAwaiting ?? false
        )
    }

    /// Every session the core holds, newest first: those it handed over this
    /// run, then the ended ones from its history.
    func entries() -> [FundingEntry] {
        guard let runtime else { return [] }

        let book = runtime.fundingEntries()
        let held = Set(book.map(\.session.intent))
        let inBook = book.map { item in
            FundingEntry(
                id: item.session.intent,
                direction: item.session.direction,
                amount: Balance(units: item.session.amount),
                session: item.session,
                progress: item.progress,
                summary: item.session.stage.isOpen ? nil : runtime.fundingSummary(intent: item.session.intent),
                options: nil,
                quotes: nil,
                isQuoting: false
            )
        }
        let past = runtime.fundingHistory().filter { !held.contains($0.intent) }.map { summary in
            FundingEntry(
                id: summary.intent,
                direction: summary.direction,
                amount: Balance(units: summary.amount),
                session: nil,
                progress: nil,
                summary: summary,
                options: nil,
                quotes: nil,
                isQuoting: false
            )
        }
        return (inBook + past).sorted { $0.openedAtMs > $1.openedAtMs }
    }

    func publish(_ entryId: String) {
        let followers = state.withLock { $0.entryFollowers[entryId].map { Array($0.values) } ?? [] }
        guard !followers.isEmpty else { return }

        let current = entry(entryId)
        for follower in followers {
            follower.yield(current)
            if current.hasEnded { follower.finish() }
        }
    }

    func publishEntries() {
        let followers = state.withLock { Array($0.listFollowers.values) }
        guard !followers.isEmpty else { return }

        let current = entries()
        for follower in followers {
            follower.yield(current)
        }
    }

    func finishFollowers(of entryId: String) {
        let followers = state.withLock { $0.entryFollowers.removeValue(forKey: entryId).map { Array($0.values) } ?? [] }
        for follower in followers {
            follower.finish()
        }
    }
}
