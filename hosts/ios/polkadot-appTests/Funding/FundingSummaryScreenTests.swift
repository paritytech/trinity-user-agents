import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The summary hands the session to the provider whose quote is on screen,
/// once, and says why when the provider would not take it. The providers
/// list changes which quote that is.
@MainActor
@Suite(.timeLimit(.minutes(1)))
struct FundingSummaryScreenTests {
    /// What the provider's refusal to take the session looks like.
    enum StartFailure: CaseIterable, Sendable {
        case providerDeclined
        case providerRefusedWithoutReason
        case unexpected
        case userLeft
    }

    @Test(arguments: [
        (nil, "ramp"),
        ("moon", "moon"),
        ("sun", "ramp")
    ] as [(String?, String)])
    func startHandsTheSessionToTheChosenQuoteElseTheBest(chosen: String?, started: String) async {
        let harness = FundingScreenHarness(entry: quotedEntry())
        harness.flowState.update { $0.chosenProviderId = chosen.map(FundingFixtures.provider) }
        let presenter = harness.summary()
        presenter.setup()

        presenter.start()
        await harness.wireframe.changes.wait { !harness.wireframe.calls.isEmpty }

        #expect(harness.service.calls == [.start(FundingFixtures.provider(started))])
        #expect(harness.wireframe.calls == [.showProgress])
    }

    @Test
    func startWaitsForAQuote() throws {
        let entry = FundingFixtures.selecting(quotes: FundingFixtures.quotes([("ramp", .pending)], best: nil))
        let harness = FundingScreenHarness(entry: entry)
        let presenter = harness.summary()
        presenter.setup()

        presenter.start()

        #expect(harness.service.calls.isEmpty)
        let viewModel = try #require(harness.view.summary)
        #expect(!viewModel.hasQuote)
        #expect(!viewModel.isStarting)
    }

    @Test(arguments: StartFailure.allCases)
    func aStartTheProviderRefusesSaysWhyAndStaysOnTheSummary(failure: StartFailure) async throws {
        let harness = FundingScreenHarness(entry: quotedEntry())
        harness.service.startResult = .failure(failure.error)
        let presenter = harness.summary()
        presenter.setup()

        presenter.start()
        #expect(harness.view.summary?.isStarting == true)
        await harness.view.changes.wait {
            !harness.service.calls.isEmpty && harness.view.summary?.isStarting == false
        }

        #expect(try #require(harness.view.summary).problem == failure.problem)
        #expect(harness.wireframe.calls.isEmpty)
    }

    @Test
    func aSecondTapWhileStartingHandsTheSessionOverOnce() async {
        let harness = FundingScreenHarness(entry: quotedEntry())
        harness.service.holdsCalls = true
        let presenter = harness.summary()
        presenter.setup()

        presenter.start()
        await harness.service.changes.wait { harness.service.heldCallCount == 1 }
        presenter.start()
        harness.service.releaseHeldCalls()
        await harness.wireframe.changes.wait { !harness.wireframe.calls.isEmpty }

        #expect(harness.service.calls == [.start(FundingFixtures.provider("ramp"))])
        #expect(harness.wireframe.calls == [.showProgress])
    }

    @Test
    func choosingAProviderPutsItsQuoteOnTheSummary() throws {
        let harness = FundingScreenHarness(entry: quotedEntry())
        let providers = harness.providers()
        providers.setup()

        providers.chooseProvider(FundingFixtures.provider("moon"))

        let selected = try #require(harness.view.providers).rows.filter(\.isSelected).map(\.providerId)
        #expect(selected == [FundingFixtures.provider("moon")])
        #expect(harness.wireframe.toasts.count == 1)
        #expect(harness.wireframe.calls.last == .back)
    }
}

private extension FundingSummaryScreenTests {
    /// Ramp quotes best, moon quotes too, and sun has not answered.
    func quotedEntry() -> FundingEntry {
        FundingFixtures.selecting(quotes: FundingFixtures.quotes(
            [
                ("ramp", .quoted(quote: FundingFixtures.quote("ramp"))),
                ("moon", .quoted(quote: FundingFixtures.quote("moon", send: "5300"))),
                ("sun", .pending)
            ],
            best: "ramp"
        ))
    }
}

private struct UnexpectedStartError: Error {}

private extension FundingSummaryScreenTests.StartFailure {
    var error: Error {
        switch self {
        case .providerDeclined: FundingStartError.notSelected(.declined)
        case .providerRefusedWithoutReason: FundingStartError.notSelected(nil)
        case .unexpected: UnexpectedStartError()
        case .userLeft: FundingStartError.left
        }
    }

    /// The problem line the summary shows; nil once the user left, when
    /// nothing more is shown.
    @MainActor var problem: String? {
        switch self {
        case .providerDeclined: FundingFailure.declined.failedText
        case .providerRefusedWithoutReason,
             .unexpected: String(localized: .Funding.errorStartFailed)
        case .userLeft: nil
        }
    }
}
