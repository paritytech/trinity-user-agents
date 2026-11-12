import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The service shows what the core decides: only the latest ask's quotes,
/// the start the core made on its best quote, and the screens it asks for.
struct FundingServiceTests {
    @Test
    func theStateShowsTheCoresListForTheAsk() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        service.select(selection(.card), for: intent)

        service.quote(amount: "50000000", for: intent)
        try await waitUntil { runtime.asks.count == 1 }
        push(quotes(1, [("routed", quote("q1", send: "5200"))], best: "routed"), runtime, service)

        try await waitUntil { await state(service).quotes?.requestId == 1 }
        let shown = await state(service)
        #expect(shown.quotes?.rows.map(\.providerId) == [provider("routed")])
        #expect(!shown.isQuoting)
    }

    @Test
    func aListForAnEarlierAskIsDropped() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        service.select(selection(.card), for: intent)

        service.quote(amount: "50000000", for: intent)
        service.quote(amount: "60000000", for: intent)
        try await waitUntil { runtime.asks.count == 2 }

        push(quotes(1, [("routed", quote("old", send: "1"))], best: "routed"), runtime, service)
        #expect(await state(service).quotes == nil)

        push(quotes(2, [("routed", quote("new", send: "1"))], best: "routed"), runtime, service)
        try await waitUntil { await state(service).quotes?.requestId == 2 }
        #expect(await state(service).quotes?.requestId == 2)
    }

    /// Lists can arrive before the core's id for their ask does: none is
    /// shown until the id comes back, and then only the latest ask's.
    @Test
    func listsArrivingBeforeTheirAsksIdShowOnlyTheLatestAsk() {
        var request = FundingQuoteRequest()
        _ = request.ask()
        let latest = request.ask()

        let earlyLatest = request.shows(quotes(2, [], best: nil))
        let earlyEarlier = request.shows(quotes(1, [], best: nil))
        let kept = request.take(2, ofAsk: latest)
        let latestShown = request.shows(quotes(2, [], best: nil))
        let earlierShown = request.shows(quotes(1, [], best: nil))

        #expect(!earlyLatest && !earlyEarlier)
        #expect(kept?.requestId == 2)
        #expect(latestShown)
        #expect(!earlierShown)
    }

    @Test
    func anUnchangedAskKeepsTheShownList() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        service.select(selection(.card), for: intent)
        service.quote(amount: "50000000", for: intent)
        try await waitUntil { runtime.asks.count == 1 }
        push(quotes(1, [("routed", quote("q1", send: "5200"))], best: "routed"), runtime, service)
        try await waitUntil { await state(service).quotes != nil }

        service.quote(amount: "50000000", for: intent)
        try await waitUntil { await !state(service).isQuoting }

        #expect(runtime.asks.count == 1)
        #expect(await state(service).quotes?.requestId == 1)
    }

    @Test
    func theCoreWithdrawingTheListShowsNone() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        service.select(selection(.card), for: intent)
        service.quote(amount: "50000000", for: intent)
        try await waitUntil { runtime.asks.count == 1 }
        push(quotes(1, [("routed", quote("q1", send: "5200"))], best: "routed"), runtime, service)
        try await waitUntil { await state(service).quotes != nil }

        push(nil, runtime, service)

        #expect(await state(service).quotes == nil)
    }

    @Test
    func noAmountDropsTheQuotes() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        service.select(selection(.card), for: intent)
        service.quote(amount: "50000000", for: intent)
        try await waitUntil { runtime.asks.count == 1 }
        push(quotes(1, [("routed", quote("q1", send: "5200"))], best: "routed"), runtime, service)
        try await waitUntil { await state(service).quotes != nil }

        service.quote(amount: nil, for: intent)

        #expect(await state(service).quotes == nil)
        #expect(await !state(service).isQuoting)
    }

    @Test
    func theAmountIssueIsTheCores() {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)

        runtime.answerAmounts(with: .belowMinimum(min: "10000000"))
        #expect(service.amountIssue("5000000", spendable: nil, for: intent) == .belowMinimum(min: "10000000"))
    }

    @Test
    func aPresentedSessionIsAskedForAndStartsOnTheChosenQuote() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        let outcome = Task { await service.presentFunding(intent: intent, direction: .in, amount: nil) }
        let requested = try await firstEvent(of: service)
        service.select(selection(.card), for: intent)
        service.quote(amount: "50000000", for: intent)
        try await waitUntil { runtime.asks.count == 1 }
        push(quotes(1, [("routed", quote("q1", send: "5200"))], best: "routed"), runtime, service)
        try await waitUntil { await state(service).quotes != nil }

        try await service.start(intent, provider: provider("routed"))

        guard case let .presentationRequested(entry) = requested else {
            Issue.record("expected a presentation request, got \(requested)")
            return
        }
        #expect(entry.id == intent)
        #expect(await outcome.value == .started)
        #expect(runtime.selectedProviders == [provider("routed")])
    }

    @Test
    func leavingWhileTheProviderIsSelectedReportsNoStart() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let service = makeService(runtime)
        let outcome = Task { await service.presentFunding(intent: intent, direction: .in, amount: nil) }
        _ = try await firstEvent(of: service)
        service.select(selection(.card), for: intent)
        service.quote(amount: "50000000", for: intent)
        try await waitUntil { runtime.asks.count == 1 }
        push(quotes(1, [("routed", quote("q1", send: "5200"))], best: "routed"), runtime, service)
        try await waitUntil { await state(service).quotes != nil }
        runtime.holdSelection()

        let start = Task { try await service.start(intent, provider: provider("routed")) }
        try await waitUntil { runtime.selectedProviders.count == 1 }
        let cancelled = try await service.cancel(intent)
        runtime.releaseSelection()

        #expect(cancelled == .ended)
        #expect(await outcome.value == .dismissed)
        await #expect(throws: FundingStartError.left) { try await start.value }
    }

    @Test
    func leavingASessionThatNeverStartedDropsIt() async throws {
        let service = makeService(MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]))
        let outcome = Task { await service.presentFunding(intent: intent, direction: .in, amount: nil) }
        _ = try await firstEvent(of: service)

        service.leave(intent)

        #expect(await outcome.value == .dismissed)
    }

    @Test
    func theDepositScreenStartsOnTheCoresBestQuote() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("crypto", routes: [cryptoRoute()])])
        let service = makeService(runtime)
        let outcome = Task { await service.presentFunding(intent: intent, direction: .in, amount: nil) }
        _ = try await firstEvent(of: service)
        service.select(selection(.crypto, asset: "USDC", network: "polkadot"), for: intent)

        service.quote(amount: "50000000", for: intent)
        try await service.start(intent, provider: nil)
        try await waitUntil { runtime.autoStarts == [.armed(requestId: 1)] }

        runtime.hold(session(provider: "crypto"))
        service.fundingChanged(intent: intent, kind: .session)

        #expect(await outcome.value == .started)
        #expect(runtime.autoStarts == [.armed(requestId: 1)])
    }

    @Test
    func leavingTheDepositScreenKeepsTheSessionFromStarting() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("crypto", routes: [cryptoRoute()])])
        let service = makeService(runtime)
        service.select(selection(.crypto, asset: "USDC", network: "polkadot"), for: intent)
        service.quote(amount: "50000000", for: intent)
        try await service.start(intent, provider: nil)

        service.stopStarting(intent)
        try await waitUntil { runtime.autoStarts.count == 2 }

        #expect(runtime.autoStarts == [.armed(requestId: 1), .cancelled])
    }

    @Test
    func theCoreClosingAProvidersScreenAnswersIt() async throws {
        let runtime = MockFundingRuntime()
        let service = makeService(runtime)
        let outcome = Task {
            await service.presentProviderFrame(
                providerId: provider("ramp"),
                intent: intent,
                target: .app(route: "/kyc")
            )
        }
        let opened = try await firstEvent(of: service)
        guard case let .frameOpened(frame) = opened else {
            Issue.record("expected the frame, got \(opened)")
            return
        }

        service.closeProviderFrame(providerId: provider("ramp"), intent: intent)

        #expect(frame.entryId == intent)
        #expect(await outcome.value == .closed)
    }

    @Test
    func theAppAnswersAProvidersScreen() async throws {
        let runtime = MockFundingRuntime()
        let service = makeService(runtime)
        let outcome = Task {
            await service.presentProviderFrame(
                providerId: provider("ramp"),
                intent: intent,
                target: .app(route: "/kyc")
            )
        }
        guard case let .frameOpened(frame) = try await firstEvent(of: service) else {
            Issue.record("expected the frame")
            return
        }

        service.answer(frame: frame.id, with: .dismissed)

        #expect(await outcome.value == .dismissed)
    }

    @Test
    func aProvidersAskForAScreenReachesTheApp() async throws {
        let service = makeService(MockFundingRuntime())

        service.providerFrameRequested(intent: intent, providerId: provider("ramp"))

        guard case let .frameRequested(request) = try await firstEvent(of: service) else {
            Issue.record("expected the ask")
            return
        }
        #expect(request == FundingFrameRequest(entryId: intent, providerId: provider("ramp")))
    }

    @Test
    func aCrashingWorkerReachesTheApp() async throws {
        let service = makeService(MockFundingRuntime())

        service.workerCrashing(productId: provider("ramp"))

        guard case let .providerNotWorking(providerId) = try await firstEvent(of: service) else {
            Issue.record("expected the crash")
            return
        }
        #expect(providerId == provider("ramp"))
    }

    @Test
    func theListHoldsSessionsInFlightAndEnded() async throws {
        let runtime = MockFundingRuntime()
        runtime.hold(session(provider: "ramp"), progress: FundingProgressFixtures.inFlight)
        runtime.record(history: [summary("ended", openedAtMs: 5)])
        let service = makeService(runtime)

        var iterator = service.subscribeEntries().makeAsyncIterator()
        let entries = try #require(await iterator.next())

        #expect(entries.map(\.id) == ["ended", intent])
        #expect(entries.map(\.phase) == [.ended, .running])
    }
}

private enum FundingProgressFixtures {
    static let inFlight = FundingProgress(
        steps: [FundingProgressStep(step: .started, reachedAtMs: 1)],
        band: .awaitingPayment,
        lastHeardAtMs: nil,
        detail: nil,
        failedAtMs: nil,
        transactionId: nil,
        reference: nil,
        deposit: nil,
        mismatch: nil,
        retrying: false,
        payout: nil,
        refunded: nil,
        status: .waitingForTransfer,
        awaitsUserTransfer: false,
        longerAtMs: nil,
        moneyMoved: false,
        cancel: .unavailable
    )
}

private extension FundingServiceTests {
    var intent: String { "intent" }

    func makeService(_ runtime: MockFundingRuntime) -> FundingService {
        FundingService(runtime: runtime, logger: MockLogger())
    }

    func state(_ service: FundingService) async -> FundingEntry {
        var iterator = service.subscribeEntryState(intent).makeAsyncIterator()
        guard let current = try? await iterator.next() else {
            preconditionFailure("the state stream yields the current state first")
        }
        return current
    }

    func firstEvent(of service: FundingService) async throws -> FundingEvent {
        var iterator = service.events.makeAsyncIterator()
        return try #require(await iterator.next())
    }

    func push(_ list: FundingQuoteList?, _ runtime: MockFundingRuntime, _ service: FundingService) {
        runtime.list(list)
        service.fundingChanged(intent: intent, kind: .quotes)
    }

    func selection(_ rail: FundingRail, asset: String? = nil, network: String? = nil) -> FundingSelection {
        FundingSelection(rail: rail, asset: asset, network: network, country: "DE", currency: "EUR")
    }

    func provider(_ name: String) -> ProductId {
        try! ProductId("\(name).dot")
    }

    func candidate(_ name: String, routes: [FundingRoute]) -> FundingCandidate {
        FundingCandidate(
            providerId: provider(name),
            routes: routes,
            unsupported: [],
            limits: [],
            declaresRoutes: !routes.isEmpty,
            brand: nil
        )
    }

    func route(_ rail: FundingRail) -> FundingRoute {
        FundingRoute(rail: rail, directions: [.in, .out], assets: ["EUR"], networks: nil, countries: nil)
    }

    func cryptoRoute() -> FundingRoute {
        FundingRoute(rail: .crypto, directions: [.in], assets: ["USDC"], networks: ["polkadot"], countries: nil)
    }

    func quotes(
        _ requestId: UInt64,
        _ rows: [(provider: String, quote: FundingQuote)],
        best: String?
    ) -> FundingQuoteList {
        FundingQuoteList(
            requestId: requestId,
            rows: rows.map { FundingQuoteRow(providerId: provider($0.provider), state: .quoted(quote: $0.quote)) },
            best: best.map(provider),
            refreshAtMs: 0,
            amount: "0",
            refusal: nil,
            issue: nil,
            complete: !rows.isEmpty
        )
    }

    /// Open session `intent` once the core handed it to `name`.
    func session(provider name: String) -> FundingSession {
        FundingSession(
            intent: intent,
            ownerProductId: nil,
            direction: .in,
            amount: "50000000",
            stage: .open,
            openedAtMs: 1,
            deadlineMs: 86_400_000,
            acknowledged: false,
            providerId: provider(name),
            assignedAtMs: 0,
            cancel: nil,
            updates: [],
            started: [],
            choice: FundingChoice(quote: quote("q1", send: "5200"), rail: .crypto, asset: "USDC", network: "polkadot"),
            refund: nil,
            detail: nil
        )
    }

    func summary(_ id: String, openedAtMs: UInt64) -> FundingSessionSummary {
        FundingSessionSummary(
            intent: id,
            ownerProductId: nil,
            direction: .in,
            amount: "50000000",
            stage: .delivered(credited: "50000000", settledAtMs: openedAtMs + 1),
            openedAtMs: openedAtMs,
            providerId: provider("ramp"),
            choice: nil,
            status: .toppedUp,
            payout: nil,
            refunded: nil,
            transactionId: nil,
            reference: nil,
            moneyMoved: true,
            ended: .init(
                atMs: openedAtMs + 1,
                outcome: .succeeded,
                payoutState: nil,
                cashAmount: "50000000",
                differingRequested: nil,
                stoppedAt: nil,
                steps: [.started, .received, .added],
                waitingAt: nil
            )
        )
    }

    func quote(_ id: String, send: U128) -> FundingQuote {
        FundingQuote(
            quoteId: id,
            sendAmount: send,
            receiveAmount: "50000000",
            providerFee: "0",
            networkFee: "0",
            etaSecs: nil,
            expiresAt: nil,
            assetDecimals: 2
        )
    }
}
