import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The screens that change what the core prices: each tells the core of the
/// new selection before it asks for quotes on it, and the deposit screen
/// stops the core starting a session the user left.
@MainActor
struct FundingSelectionScreenTests {
    @Test
    func choosingACountryReportsItsCurrencyThenQuotesTheTypedAmount() throws {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        harness.flowState.update { $0.amountText = "50" }
        let presenter = harness.country()
        presenter.setup()

        let france = try #require(FundingCountries.country(code: "FR"))
        presenter.chooseCountry(france)

        let selection = FundingSelection(rail: .card, asset: nil, network: nil, country: "FR", currency: "EUR")
        #expect(harness.service.calls == [.select(selection), .quote(50_000_000)])
        #expect(harness.wireframe.calls == [.back])
    }

    @Test
    func aCountryTheCoreDetectsLaterKeepsTheOneTheUserPicked() throws {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        let presenter = harness.country()
        presenter.setup()

        try presenter.chooseCountry(#require(FundingCountries.country(code: "FR")))
        presenter.didReceive(entry: FundingFixtures.selecting(options: FundingFixtures.offering(detectedCountry: "US")))

        let viewModel = try #require(harness.view.country)
        #expect(viewModel.selected?.code == "FR")
        #expect(viewModel.detected?.code == "US")
    }

    @Test
    func pickingANetworkThenATokenReportsEachAndQuotesOnTheToken() {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        harness.flowState.update { draft in
            draft.rail = .crypto
            draft.asset = "DOT"
            draft.amountText = "50"
        }
        let network = harness.network()
        let token = harness.token()
        network.setup()
        token.setup()

        network.chooseNetwork("polkadot")
        token.chooseToken("USDC")

        #expect(harness.service.calls == [
            .select(cryptoSelection(asset: nil)),
            .select(cryptoSelection(asset: "USDC")),
            .quote(50_000_000)
        ])
        #expect(harness.wireframe.calls == [.show(.token), .show(.summary)])
    }

    /// Leaving the address before the session started stops the core
    /// starting it on its best quote; once started, back returns to its
    /// progress.
    @Test(arguments: [false, true])
    func backFromTheDepositStopsAnUnstartedSession(hasStarted: Bool) {
        let entry = hasStarted ? FundingFixtures.started(rail: .crypto) : FundingFixtures.selecting()
        let harness = FundingScreenHarness(entry: entry)
        let presenter = harness.deposit()
        presenter.setup()

        presenter.back()

        #expect(harness.service.calls == (hasStarted ? [] : [.stopStarting]))
        #expect(harness.wireframe.calls == (hasStarted ? [.showProgress] : [.back]))
    }
}

private extension FundingSelectionScreenTests {
    func cryptoSelection(asset: String?) -> FundingSelection {
        FundingSelection(rail: .crypto, asset: asset, network: "polkadot", country: "DE", currency: "EUR")
    }
}
