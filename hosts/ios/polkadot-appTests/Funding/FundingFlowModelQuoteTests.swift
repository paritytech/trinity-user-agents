import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The provider list follows the core's quote list: every funding provider is
/// asked, and a route only orders the list.
@MainActor
struct FundingFlowModelQuoteTests {
    @Test
    func aProviderThatQuotesWithoutAMatchingRouteIsListedAndSelectable() {
        let runtime = FakeFundingRuntime(candidates: [
            candidate("routed", routes: [route(.card)]),
            candidate("unrouted", routes: [])
        ])
        let model = makeModel(runtime)

        model.selectRail(.card)
        model.amountText = "50"
        model.requestQuote()
        model.receive(quotes: FundingQuoteList(
            requestId: runtime.lastRequestId,
            rows: [
                FundingQuoteRow(providerId: "routed", state: .quoted(quote: quote("q1", send: "5200"))),
                FundingQuoteRow(providerId: "unrouted", state: .quoted(quote: quote("q2", send: "5100")))
            ],
            best: "unrouted",
            refreshAtMs: 0
        ))

        #expect(model.listedProviderIds == ["routed", "unrouted"])
        #expect(model.selectedProviderId == "unrouted")
        #expect(model.selectedQuote?.quoteId == "q2")
    }

    @Test
    func cardAndBankAreOfferedWithoutRoutesAndCryptoNeedsOne() {
        let model = makeModel(FakeFundingRuntime(candidates: [candidate("unrouted", routes: [])]))

        #expect(model.availableRails == [.card, .bank])
    }

    @Test
    func anAmountBelowTheCoresMinimumIsRefused() {
        let model = makeModel(FakeFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]))
        model.selectRail(.card)

        model.amountText = "5"
        #expect(model.amountIssue == .belowMinimum(10))

        model.amountText = "10"
        #expect(model.amountIssue == nil)
    }

    @Test
    func aWithdrawalAboveWhatPaymentsCanSpendIsRefused() {
        let model = makeModel(
            FakeFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])]),
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
    func aListForAnEarlierAskIsDropped() {
        let runtime = FakeFundingRuntime(candidates: [candidate("routed", routes: [route(.card)])])
        let model = makeModel(runtime)
        model.selectRail(.card)
        model.amountText = "50"
        model.requestQuote()
        let earlier = runtime.lastRequestId
        model.requestQuote()

        model.receive(quotes: FundingQuoteList(
            requestId: earlier,
            rows: [FundingQuoteRow(providerId: "routed", state: .quoted(quote: quote("old", send: "1")))],
            best: "routed",
            refreshAtMs: 0
        ))

        #expect(model.selectedQuote == nil)
    }
}

private extension FundingFlowModelQuoteTests {
    func makeModel(_ runtime: FakeFundingRuntime, direction: FundingDirection = .in) -> FundingFlowModel {
        FundingFlowModel(
            context: FundingFlowModel.Context(
                intent: "intent",
                direction: direction,
                amount: nil,
                cash: FundingCash(symbol: "CASH", precision: 6)
            ),
            runtime: runtime,
            branding: NoBranding()
        )
    }

    func candidate(_ providerId: String, routes: [FundingRoute]) -> FundingCandidate {
        FundingCandidate(providerId: providerId, routes: routes, unsupported: [], limits: [])
    }

    func route(_ rail: FundingRail) -> FundingRoute {
        FundingRoute(rail: rail, directions: [.in, .out], assets: ["EUR"], networks: nil, countries: nil)
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

private struct NoBranding: FundingProviderBranding {
    func brand(for providerId: String) async -> FundingProviderBrand {
        .placeholder(for: providerId)
    }
}

/// A runtime holding only what the quote path reads.
private final class FakeFundingRuntime: FundingRuntime, @unchecked Sendable {
    private let candidates: [FundingCandidate]
    private(set) var lastRequestId: UInt64 = 0

    init(candidates: [FundingCandidate]) {
        self.candidates = candidates
    }

    func fundingSession(intent _: String) -> FundingSession? { nil }
    func fundingProgress(intent _: String) -> FundingProgress? { nil }
    func fundingSessions() -> [FundingSession] { [] }
    func fundingHistory() -> [FundingSessionSummary] { [] }
    func fundingInProgress() async throws -> Bool { false }
    func fundingCandidates(intent _: String) -> [FundingCandidate] { candidates }

    func getFundingQuote(intent _: String, ask _: FundingQuoteAsk) -> UInt64 {
        lastRequestId += 1
        return lastRequestId
    }

    func stopFundingQuotes(intent _: String) {}
    func fundingCountries(intent _: String, rail _: FundingRail, asset _: String) -> [FundingCountry] { [] }

    func fundingAmountLimits(
        intent _: String,
        rail _: FundingRail,
        asset _: String,
        network _: String?,
        country _: String?
    ) -> FundingAmountLimits {
        FundingAmountLimits(min: "10000000", max: nil)
    }

    func detectedFundingCountry() async -> String? { nil }
    func selectFundingProvider(intent _: String, providerId _: String, quoteId _: String?) async throws -> Bool { true }
    func cancelFunding(intent _: String) async throws -> CancelOutcome { .ended }
    func confirmFundingCancel(intent _: String, terms _: CancelTerms?) async throws -> CancelConfirmation { .declined }
    func acknowledgeFundingSession(intent _: String) async throws -> Bool { true }
    func openFunding(direction _: FundingDirection, amount _: U128?) async throws -> String? { nil }
}
