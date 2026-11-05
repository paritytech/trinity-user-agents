import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The provider list follows the core's quote list: every funding provider is
/// asked, and a route only orders the list.
@MainActor
struct FundingFlowModelQuoteTests {
    @Test
    func aProviderThatQuotesWithoutAMatchingRouteIsListedAndSelectable() async throws {
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
    func cardAndBankAreOfferedWithoutRoutesAndCryptoNeedsOne() {
        let model = makeModel(MockFundingRuntime(candidates: [candidate("unrouted", routes: [])]))

        #expect(model.availableRails == [.card, .bank])
    }

    @Test
    func anAmountBelowTheCoresMinimumIsRefused() {
        let model = makeModel(MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]))
        model.selectRail(.card)

        model.amountText = "5"
        #expect(model.amountIssue == .belowMinimum(10))

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
        try await waitUntil { runtime.asks.count == 2 }
        try await Task.sleep(for: .milliseconds(100))

        model.receive(quotes: quotes(1, [("routed", quote("old", send: "1"))], best: "routed"))

        #expect(model.selectedQuote == nil)
    }

    @Test
    func continuingCancelsTheQuoteWaitingOnThePauseAndAsksNothingTwice() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let model = makeModel(runtime)
        model.selectRail(.card)

        model.amountText = "50"
        model.amountChanged()
        try await waitUntil { runtime.asks.count == 1 }
        model.continueFromAmount()

        model.back()
        model.amountText = "60"
        model.amountChanged()
        model.continueFromAmount()
        try await Task.sleep(for: .milliseconds(800))

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
    func choosingACountryReportsItsCurrency() async throws {
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
        #expect(reported.allSatisfy { $0.asset == "USD" })
        #expect(runtime.asks.last?.asset == "USD")
    }

    @Test
    func aBrandIsReadOnceWhileItLoads() async throws {
        let runtime = MockFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let branding = MockFundingBranding()
        let model = makeModel(runtime, branding: branding)

        model.refreshOptions()
        model.refreshOptions()
        try await waitUntil { model.brand(for: provider("routed")).name == "routed.dot" }
        try await Task.sleep(for: .milliseconds(100))

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
            service: FundingFlowService(runtime: runtime, book: runtime.book, branding: branding),
            logger: MockLogger()
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
        best: String
    ) -> FundingQuoteList {
        FundingQuoteList(
            requestId: requestId,
            rows: rows.map { FundingQuoteRow(providerId: provider($0.provider), state: .quoted(quote: $0.quote)) },
            best: provider(best),
            refreshAtMs: 0
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
