import Foundation
import SubstrateSdk
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The amount screen: the core decides whether the typed amount can go
/// ahead, quotes are asked once the keypad pauses, and Continue leads on by
/// rail.
@MainActor
@Suite(.timeLimit(.minutes(1)))
struct FundingAmountScreenTests {
    private let cardSelection = FundingSelection(rail: .card, asset: nil, network: nil, country: "DE", currency: "EUR")

    @Test
    func anAmountTheCoreRejectsBlocksContinue() throws {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        harness.service.amountIssueAnswer = .belowMinimum(min: "10000000")
        let presenter = harness.amount()
        presenter.setup()

        presenter.amountTextChanged("5")
        presenter.continueFromAmount()

        let viewModel = try #require(harness.view.amount)
        #expect(!viewModel.canContinue)
        #expect(viewModel.issue != nil)
        #expect(harness.wireframe.calls.isEmpty)
    }

    @Test(arguments: ["", "0", "0.00", "1.2.3", "1e5"])
    func textThatIsNoPositiveAmountBlocksContinue(text: String) throws {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        let presenter = harness.amount()
        presenter.setup()

        presenter.amountTextChanged(text)
        presenter.continueFromAmount()

        #expect(try !#require(harness.view.amount).canContinue)
        #expect(harness.wireframe.calls.isEmpty)
        #expect(harness.service.calls == [.select(cardSelection)])
    }

    /// The host never asks for more than the user typed: the amount is
    /// rounded down to the payment asset's six decimals.
    @Test(arguments: [
        ("50", 50_000_000),
        ("0.01", 10_000),
        ("1.2345678", 1_234_567)
    ] as [(String, UInt64)])
    func continuingOnCardQuotesTheTypedAmountOnTheReportedSelection(text: String, quoted: UInt64) {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        let presenter = harness.amount()
        presenter.setup()

        presenter.amountTextChanged(text)
        presenter.continueFromAmount()

        #expect(harness.service.calls == [.select(cardSelection), .quote(Balance(quoted))])
        #expect(harness.wireframe.calls == [.show(.summary)])
    }

    @Test
    func continuingOnCryptoPicksTheNetworkBeforeAnyQuote() {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        let presenter = harness.amount()
        presenter.setup()

        presenter.selectRail(.crypto)
        presenter.amountTextChanged("50")
        presenter.continueFromAmount()

        #expect(!harness.service.calls.contains(.quote(50_000_000)))
        #expect(harness.wireframe.calls == [.show(.network)])
    }

    @Test
    func typingAsksForQuotesOnceTheKeypadPauses() async {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        let presenter = harness.amount()
        presenter.setup()

        presenter.amountTextChanged("5")
        presenter.amountTextChanged("50")
        await harness.clock.resumeSleep(for: FundingAmountPresenter.quotePause)
        await harness.service.changes.wait { harness.service.calls.count == 2 }

        #expect(harness.service.calls == [.select(cardSelection), .quote(50_000_000)])
        #expect(harness.wireframe.calls.isEmpty)
    }

    @Test
    func switchingRailDropsTheQuotesAndAsksAgainOnTheNewRail() async {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting())
        let presenter = harness.amount()
        presenter.setup()

        presenter.amountTextChanged("50")
        presenter.selectRail(.bank)
        await harness.clock.resumeSleep(for: FundingAmountPresenter.quotePause)
        await harness.service.changes.wait { harness.service.calls.count == 4 }

        let bankSelection = FundingSelection(rail: .bank, asset: nil, network: nil, country: "DE", currency: "EUR")
        #expect(harness.service.calls == [
            .select(cardSelection),
            .quote(nil),
            .select(bankSelection),
            .quote(50_000_000)
        ])
    }

    @Test
    func aWithdrawalIsCheckedAgainstWhatAPaymentCanSpend() async throws {
        let harness = FundingScreenHarness(entry: FundingFixtures.selecting(.out))
        harness.environment.spendable = 20_000_000
        harness.service.amountIssueAnswer = .notEnoughBalance
        let presenter = harness.amount()
        presenter.setup()
        await harness.view.changes.wait { harness.flowState.draft.spendable != nil }

        presenter.amountTextChanged("50")
        presenter.continueFromAmount()

        #expect(harness.service.spendableAsked.last == 20_000_000)
        #expect(try !#require(harness.view.amount).canContinue)
        #expect(harness.wireframe.calls.isEmpty)
    }
}
