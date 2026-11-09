import Clocks
import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The overlay draws what the core decides: the providers it asked in its
/// order, the rails it offers, the asset an ask names and why an amount
/// cannot go ahead.
@MainActor
struct FundingFlowModelQuoteTests {
    @Test
    func theListFollowsTheCoresQuoteList() async throws {
        let runtime = MockFundingRuntime(candidates: [
            candidate("routed", routes: [route(.card)]),
            candidate("unrouted", routes: [])
        ])
        let model = makeModel(runtime)

        model.selectRail(.card)
        model.amountText = "50"
        model.continueFromAmount()
        try await waitUntil { runtime.asks.count == 1 }
        model.receive(quotes: quotes(
            runtime.lastRequestId,
            [("routed", quote("q1", send: "5200")), ("unrouted", quote("q2", send: "5100"))],
            best: "unrouted"
        ))

        #expect(model.listedProviderIds == [provider("routed"), provider("unrouted")])
        #expect(model.selectedProviderId == provider("unrouted"))
        #expect(model.selectedQuote?.quoteId == "q2")
    }

    @Test
    func offersTheRailsTheCoreRoutesAndStartsOnItsDefault() {
        let model = makeModel(MockFundingRuntime(
            candidates: [candidate("unrouted", routes: [])],
            rails: [.crypto, .card]
        ))

        #expect(model.availableRails == [.crypto, .card])
        #expect(model.rail == .crypto)
    }

    @Test
    func theTypedAmountIsReadAsTheCoreReadsIt() {
        let model = makeModel(MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]))

        model.amountText = "50.1234567"
        #expect(model.amountUnits == "50123456")

        model.amountText = "5x"
        #expect(model.amountUnits == nil)
        #expect(model.amount == 0)
    }

    @Test
    func anAmountBelowTheCoresMinimumIsRefused() {
        let model = makeModel(MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]))
        model.selectRail(.card)

        model.amountText = "5"
        #expect(model.amountIssue == .belowMinimum(min: "10000000"))

        model.amountText = "10"
        #expect(model.amountIssue == nil)
    }

    @Test
    func aWithdrawalAboveWhatPaymentsCanSpendIsRefused() {
        let model = makeModel(
            MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]),
            direction: .out
        )
        model.selectRail(.card)
        model.spendable = 20

        model.amountText = "30"
        #expect(model.amountIssue == .notEnoughBalance)

        model.amountText = "15"
        #expect(model.amountIssue == nil)
    }

    @Test
    func aListForAnEarlierAskIsDropped() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let model = makeModel(runtime)
        model.selectRail(.card)
        model.amountText = "50"
        model.continueFromAmount()
        model.back()
        model.amountText = "60"
        model.continueFromAmount()
        await model.settled()
        #expect(runtime.asks.count == 2)

        model.receive(quotes: quotes(1, [("routed", quote("old", send: "1"))], best: "routed"))
        #expect(model.selectedQuote == nil)

        model.receive(quotes: quotes(2, [("routed", quote("new", send: "1"))], best: "routed"))
        #expect(model.selectedQuote?.quoteId == "new")
    }

    /// Lists can reach the overlay before the core's id for their ask does:
    /// none is shown until the id comes back, and then only the latest ask's.
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
    func everyProviderRefusingTheAmountIsItsIssue() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let model = makeModel(runtime)
        model.selectRail(.card)
        model.amountText = "50"
        model.continueFromAmount()
        await model.settled()

        var list = quotes(runtime.lastRequestId, [], best: nil)
        list.amount = "50000000"
        list.issue = .aboveMaximum(max: "40000000")
        model.receive(quotes: list)

        #expect(model.amountIssue == .aboveMaximum(max: "40000000"))
        #expect(model.unavailableIssue == nil)
    }

    /// The core keeps refreshing the last ask after the user changes rail;
    /// those lists are not shown on the new rail.
    @Test
    func changingRailDropsTheRefreshesOfTheEarlierAsk() async throws {
        let runtime = MockFundingRuntime(candidates: [
            candidate("routed", routes: [route(.card)]),
            candidate("crypto", routes: [cryptoRoute()])
        ])
        let model = makeModel(runtime)
        model.selectRail(.card)
        model.amountText = "50"
        model.continueFromAmount()
        await model.settled()
        model.receive(quotes: quotes(runtime.lastRequestId, [("routed", quote("q1", send: "5200"))], best: "routed"))
        #expect(model.selectedQuote?.quoteId == "q1")

        model.back()
        model.selectRail(.crypto)
        model.receive(quotes: quotes(runtime.lastRequestId, [("routed", quote("q2", send: "5200"))], best: "routed"))

        #expect(model.selectedQuote == nil)
        #expect(model.bestProviderId == nil)
    }

    @Test
    func continuingCancelsTheQuoteWaitingOnThePauseAndAsksNothingTwice() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let model = makeModel(runtime)
        model.selectRail(.card)

        model.amountText = "50"
        model.amountChanged()
        await model.settled()
        #expect(runtime.asks.count == 1)
        model.continueFromAmount()

        model.back()
        model.amountText = "60"
        model.amountChanged()
        model.continueFromAmount()
        await model.settled()

        #expect(runtime.asks.map(\.amount) == ["50000000", "60000000"])
    }

    @Test
    func leavingWhileTheProviderIsSelectedReportsNoStart() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let model = makeModel(runtime)
        var outcomes: [FundingPresentOutcome] = []
        model.onOutcome = { outcomes.append($0) }
        model.selectRail(.card)
        model.amountText = "50"
        model.continueFromAmount()
        try await waitUntil { runtime.asks.count == 1 }
        model.receive(quotes: quotes(runtime.lastRequestId, [("routed", quote("q1", send: "5200"))], best: "routed"))
        runtime.holdSelection()

        model.start()
        try await waitUntil { runtime.selectedProviders.count == 1 }
        model.close()
        runtime.releaseSelection()
        try await waitUntil { !model.isStarting }

        #expect(outcomes == [.dismissed])
        #expect(!model.hasStarted)
        #expect(model.path == [.summary])
    }

    @Test
    func goingBackFromTheDepositScreenKeepsTheSessionFromStarting() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("crypto", routes: [cryptoRoute()])])
        let model = makeModel(runtime)
        model.selectRail(.crypto)
        model.amountText = "50"
        model.continueFromAmount()
        model.chooseNetwork("polkadot")
        model.chooseToken("USDC")
        model.back()
        try await waitUntil { runtime.asks.count == 1 }

        model.receive(quotes: quotes(runtime.lastRequestId, [("crypto", quote("q1", send: "5200"))], best: "crypto"))

        #expect(!model.isStarting)
        #expect(model.path == [.network, .token])
    }

    @Test
    func choosingACountryReportsItsCurrencyAndAsksForTheCoresAsset() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("unrouted", routes: [])])
        let model = makeModel(runtime)
        model.path.append(.country)
        model.chooseCountry(country("DE", currency: "EUR"))
        model.amountText = "50"
        model.continueFromAmount()

        model.path.append(.country)
        model.chooseCountry(country("US", currency: "USD"))
        try await waitUntil { runtime.asks.count == 2 }

        let reported = runtime.selections.filter { $0.country == "US" }
        #expect(!reported.isEmpty)
        #expect(reported.allSatisfy { $0.currency == "USD" })
        #expect(reported.allSatisfy { $0.asset == nil })
        #expect(runtime.asks.first?.asset == "EUR")
        #expect(runtime.asks.last?.asset == "USD")
    }

    @Test
    func aBrandIsReadOnceWhileItLoads() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let branding = MockFundingBranding()
        let model = makeModel(runtime, branding: branding)

        model.refreshOptions()
        model.refreshOptions()
        await model.settled()

        #expect(model.brand(for: provider("routed")).name == "routed.dot")
        #expect(branding.readProviders == [provider("routed")])
    }
}

private extension FundingFlowModelQuoteTests {
    func makeModel(
        _ runtime: MockFundingRuntime,
        direction: FundingDirection = .in,
        branding: FundingProviderBranding = MockFundingBranding()
    ) -> FundingFlowModel {
        FundingFlowModel(
            context: FundingFlowModel.Context(
                intent: "intent",
                direction: direction,
                amount: nil,
                cash: FundingCash(symbol: "CASH", precision: 6)
            ),
            service: FundingFlowService(runtime: runtime, branding: branding),
            logger: MockLogger(),
            clock: ImmediateClock()
        )
    }

    func provider(_ name: String) -> ProductId {
        try! ProductId("\(name).dot")
    }

    func candidate(_ name: String, routes: [FundingRoute]) -> FundingCandidate {
        FundingCandidate(providerId: provider(name), routes: routes, unsupported: [], limits: [])
    }

    func route(_ rail: FundingRail) -> FundingRoute {
        FundingRoute(rail: rail, directions: [.in, .out], assets: ["EUR"], networks: nil, countries: nil)
    }

    func cryptoRoute() -> FundingRoute {
        FundingRoute(rail: .crypto, directions: [.in], assets: ["USDC"], networks: ["polkadot"], countries: nil)
    }

    func country(_ code: String, currency: String) -> PaymentCountry {
        PaymentCountry(code: code, name: code, currencyCode: currency, currencyName: nil)
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
            issue: nil
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
