import Foundation
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

private let provider = try! ProductId("provider.dot")

/// The history is the core's, each row drawn from its summary.
struct FundingHistoryTests {
    @Test
    func aSummaryKeepsItsDetails() throws {
        let item = try #require(FundingActivityItem(summary: summary("old", settledAtMs: 1_000)))
        let ended = try #require(FundingEndedSession(summary: summary("old", settledAtMs: 1_000)))

        #expect(ended.providerId == provider)
        #expect(ended.rail == .crypto)
        #expect(ended.network == "polkadot")
        #expect(ended.settledAmount == "50000000")
        #expect(ended.quotedAmount == FundingAssetUnit(code: "USDC", decimals: 2).format("4600"))
        #expect(ended.summary.transactionId == "tx")
        #expect(item.status == .toppedUp)
        #expect(item.rail == .crypto)
        #expect(item.date == Date(milliseconds: openedAtMs + 1_000))
    }

    @Test
    func theCoresEndingDecidesTheRow() throws {
        let cancelled = withdrawal(
            status: .cancelled(refunded: "40000000"),
            payout: nil,
            refunded: "40000000",
            outcome: .failed(reason: .cancelled)
        )
        let payoutFailedSummary = withdrawal(
            status: .payoutFailed(refunded: "50000000"),
            payout: .failed(reason: "rejected"),
            refunded: "50000000",
            outcome: .payoutFailed(reason: "rejected"),
            payoutState: .failed,
            stoppedAt: .sent
        )
        let sentSummary = withdrawal(
            status: .paidOutAfterCancel,
            payout: .paidOut,
            refunded: nil,
            payoutState: .paidOut
        )

        #expect(FundingActivityItem(summary: cancelled)?.status == .cancelled(refunded: "40000000"))
        #expect(FundingActivityItem(summary: payoutFailedSummary)?.status == .payoutFailed(refunded: "50000000"))
        let payoutFailedDetail = try #require(FundingEndedSession(summary: payoutFailedSummary))
        #expect(payoutFailedDetail.outcome == .payoutFailed(reason: "rejected"))
        #expect(payoutFailedDetail.stoppedAt == 2)
        #expect(FundingActivityItem(summary: sentSummary)?.status == .sent)
        #expect(FundingEndedSession(summary: sentSummary)?.payoutState == .paidOut)
    }
}

/// A provider closing its frame answers the frame `.closed` and hands over to
/// the session sheet.
@MainActor
struct FundingCloseFrameTests {
    @Test
    func closingTheFrameAnswersClosedAndShowsTheSessionSheet() async throws {
        let environment = WindowEnvironment()
        let runtime = MockFundingRuntime()
        runtime.hold(session("intent", stage: .open))
        let coordinator = FundingOverlayCoordinator(runtime: runtime, environment: environment)

        let answer = Task {
            await coordinator.presentFrame(
                providerId: provider,
                intent: "intent",
                route: "/",
                awaitsUserTransfer: false
            )
        }
        try await environment.waitUntil { $0.presented.last is FundingProviderFrameController }

        coordinator.closeFrame(providerId: provider, intent: "intent")

        #expect(await answer.value == .closed)
        try await environment.waitUntil { $0.presented.last is FundingSheetController }
    }

    @Test
    func anotherProvidersCloseLeavesTheFrameOpen() async throws {
        let environment = WindowEnvironment()
        let runtime = MockFundingRuntime()
        runtime.hold(session("intent", stage: .open))
        let coordinator = FundingOverlayCoordinator(runtime: runtime, environment: environment)

        let answer = Task {
            await coordinator.presentFrame(
                providerId: provider,
                intent: "intent",
                route: "/",
                awaitsUserTransfer: false
            )
        }
        try await environment.waitUntil { $0.presented.last is FundingProviderFrameController }

        try coordinator.closeFrame(providerId: ProductId("other.dot"), intent: "intent")
        coordinator.closeFrame(providerId: provider, intent: "elsewhere")
        let frame = try #require(environment.presented.last as? FundingProviderFrameController)
        #expect(frame.presentingViewController != nil)

        coordinator.closeFrame(providerId: provider, intent: "intent")
        #expect(await answer.value == .closed)
    }
}

private let openedAtMs: UInt64 = 1_700_000_000_000

private func delivered(_ settledAtMs: UInt64) -> FundingStage {
    .delivered(credited: "50000000", settledAtMs: openedAtMs + settledAtMs)
}

private func summary(_ intent: String, settledAtMs: UInt64) -> FundingSessionSummary {
    FundingSessionSummary(
        intent: intent,
        ownerProductId: nil,
        direction: .in,
        amount: "50000000",
        stage: delivered(settledAtMs),
        openedAtMs: openedAtMs,
        providerId: provider,
        choice: FundingChoice(quote: quote(send: "4600"), rail: .crypto, asset: "USDC", network: "polkadot"),
        status: .toppedUp,
        payout: nil,
        refunded: nil,
        transactionId: "tx",
        reference: "ref",
        moneyMoved: true,
        ended: .init(
            atMs: openedAtMs + settledAtMs,
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

private func withdrawal(
    status: FundingStatus,
    payout: FundingPayout?,
    refunded: U128?,
    outcome: FundingEndedOutcome = .succeeded,
    payoutState: FundingPayoutState? = nil,
    stoppedAt: FundingStep? = nil
) -> FundingSessionSummary {
    FundingSessionSummary(
        intent: "out",
        ownerProductId: nil,
        direction: .out,
        amount: "50000000",
        stage: .released(debited: "50000000", settledAtMs: openedAtMs + 1_000),
        openedAtMs: openedAtMs,
        providerId: provider,
        choice: FundingChoice(quote: quote(send: "50000000"), rail: .bank, asset: "EUR", network: nil),
        status: status,
        payout: payout,
        refunded: refunded,
        transactionId: nil,
        reference: nil,
        moneyMoved: true,
        ended: .init(
            atMs: openedAtMs + 1_000,
            outcome: outcome,
            payoutState: payoutState,
            cashAmount: "50000000",
            differingRequested: nil,
            stoppedAt: stoppedAt,
            steps: [.started, .received, .sent],
            waitingAt: nil
        )
    )
}

private func quote(send: U128) -> FundingQuote {
    FundingQuote(
        quoteId: "quote",
        sendAmount: send,
        receiveAmount: "50000000",
        providerFee: "0",
        networkFee: "0",
        etaSecs: nil,
        expiresAt: nil,
        assetDecimals: 2
    )
}

private func session(_ intent: String, stage: FundingStage) -> FundingSession {
    FundingSession(
        intent: intent,
        ownerProductId: nil,
        direction: .in,
        amount: "50000000",
        stage: stage,
        openedAtMs: openedAtMs,
        deadlineMs: openedAtMs + 86_400_000,
        acknowledged: false,
        providerId: provider,
        assignedAtMs: openedAtMs,
        cancel: nil,
        updates: [],
        started: [],
        choice: FundingChoice(quote: quote(send: "50000000"), rail: .card, asset: "EUR", network: nil),
        refund: nil,
        detail: nil
    )
}

/// Presents for real, over a window of its own, so frames go on and off
/// screen as they do in the app.
@MainActor
private final class WindowEnvironment: FundingOverlayEnvironment {
    private let window: UIWindow
    private(set) var presented: [UIViewController] = []
    /// Whether anything is on screen to present over.
    var presents = true

    init() {
        window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
    }

    var cash: FundingCash { FundingCash(symbol: "CASH", precision: 6) }
    var branding: FundingProviderBranding { PlaceholderBranding() }

    func followSpendable(_: @escaping @MainActor (Decimal) -> Void) async {}

    func present(_ controller: UIViewController) -> Bool {
        guard presents, var top = window.rootViewController else { return false }
        while let next = top.presentedViewController, !next.isBeingDismissed {
            top = next
        }
        top.present(controller, animated: false)
        presented.append(controller)
        return true
    }

    func providerPage(providerId _: ProductId, route _: String, title _: String) -> UIViewController? {
        UIViewController()
    }

    func fundingActivityChanged() {}

    private(set) var notices: [String] = []

    func showNotice(_ message: String) {
        notices.append(message)
    }

    func waitUntil(_ condition: @escaping @MainActor (WindowEnvironment) -> Bool) async throws {
        try await withTimeout(.hangGuard) { @MainActor [self] in
            while !condition(self) {
                try await Task.sleep(for: .milliseconds(20))
            }
        }
    }
}

private struct PlaceholderBranding: FundingProviderBranding {
    func brand(for providerId: ProductId) async -> FundingProviderBrand {
        .placeholder(for: providerId)
    }
}

/// An ended session shown again is drawn from its summary and acknowledged
/// once its sheet is on screen, and not when nothing could present it.
@MainActor
struct FundingReopenTests {
    @Test
    func anEndedSessionIsAcknowledgedOnceItsSheetIsShown() async throws {
        let environment = WindowEnvironment()
        let runtime = MockFundingRuntime()
        runtime.hold(session("intent", stage: delivered(2_000)))
        runtime.record(history: [summary("intent", settledAtMs: 2_000)])
        let coordinator = FundingOverlayCoordinator(runtime: runtime, environment: environment)

        coordinator.reopen(intent: "intent")

        #expect(environment.presented.last is FundingEndedSheetController)
        try await environment.waitUntil { _ in runtime.acknowledged == ["intent"] }
    }

    @Test
    func aSheetThatCannotBePresentedLeavesTheSessionUnacknowledged() async throws {
        let environment = WindowEnvironment()
        environment.presents = false
        let runtime = MockFundingRuntime()
        runtime.hold(session("intent", stage: delivered(2_000)))
        runtime.record(history: [summary("intent", settledAtMs: 2_000)])
        let coordinator = FundingOverlayCoordinator(runtime: runtime, environment: environment)

        coordinator.reopen(intent: "intent")
        try await Task.sleep(for: .milliseconds(200))

        #expect(environment.presented.isEmpty)
        #expect(runtime.acknowledged.isEmpty)
    }
}

/// A worker that keeps crashing is named to the user while the core retries.
@MainActor
struct FundingWorkerCrashingTests {
    @Test
    func aCrashingProviderIsNamedOverWhateverIsOnScreen() async {
        let environment = WindowEnvironment()
        let coordinator = FundingOverlayCoordinator(runtime: MockFundingRuntime(), environment: environment)

        await coordinator.workerCrashing(productId: try! ProductId("ramp.dot"))

        #expect(environment.notices == [String(localized: .Funding.providerCrashing(provider: "Ramp"))])
    }
}
