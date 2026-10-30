import Foundation
import os
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The history is the core's, read each time it is drawn; an ended session is
/// acknowledged once the user has been shown it.
struct FundingHistoryTests {
    @Test
    func aSummaryAloneIsListedFromTheCoresHistory() throws {
        let records = FundingActivityCenter.historyRecords(
            summaries: [summary("old", settledAtMs: 1_000)],
            ended: []
        )

        let record = try #require(records.first)
        #expect(records.map(\.intent) == ["old"])
        #expect(record.settledAmount == "50000000")
    }

    @Test
    func aSessionFromAnEarlierRunKeepsItsDetails() throws {
        let record = try #require(FundingRecord(summary: summary("old", settledAtMs: 1_000)))
        let item = FundingActivityItem(record: record)

        #expect(record.providerId == "provider")
        #expect(record.rail == .card)
        #expect(record.asset == "EUR")
        #expect(record.transactionId == "tx")
        #expect(record.reference == "ref")
        #expect(item.status == .toppedUp)
        #expect(item.rail == .card)
    }

    @Test
    func theSummarysStatusDecidesHowItEnded() throws {
        let cancelled = try #require(FundingRecord(
            summary: withdrawal(status: .cancelled(refunded: "40000000"), payout: nil, refunded: "40000000")
        ))
        let payoutFailed = try #require(FundingRecord(
            summary: withdrawal(
                status: .payoutFailed(refunded: "50000000"),
                payout: .failed(reason: "rejected"),
                refunded: "50000000"
            )
        ))
        let sent = try #require(FundingRecord(
            summary: withdrawal(status: .paidOutAfterCancel, payout: .paidOut, refunded: nil)
        ))

        #expect(FundingActivityItem(record: cancelled).status == .cancelled(refunded: "40000000"))
        #expect(FundingActivityItem(record: payoutFailed).status == .payoutFailed(refunded: "50000000"))
        #expect(FundingEndedSession(record: payoutFailed).outcome == .payoutFailed(reason: "rejected"))
        #expect(FundingActivityItem(record: sent).status == .sent)
        #expect(FundingEndedSession(record: sent).payoutState == .paidOut)
    }

    @Test
    func aSessionPushedThisRunKeepsItsDetails() throws {
        let pushed = try #require(FundingRecord(session: session("intent", stage: delivered(2_000)), progress: nil))

        let records = FundingActivityCenter.historyRecords(
            summaries: [summary("intent", settledAtMs: 2_000)],
            ended: [pushed]
        )

        #expect(records == [pushed])
        #expect(records.first?.providerId == "provider")
    }

    @Test
    func anEndedSessionTheHistoryDoesNotListYetIsKeptNewestFirst() throws {
        let pushed = try #require(FundingRecord(session: session("new", stage: delivered(3_000)), progress: nil))

        let records = FundingActivityCenter.historyRecords(
            summaries: [summary("old", settledAtMs: 1_000)],
            ended: [pushed]
        )

        #expect(records.map(\.intent) == ["new", "old"])
    }

    @Test
    func anEndedSessionIsAcknowledgedOnce() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime()
        book.record(intent: "intent", session: session("intent", stage: delivered(2_000)), progress: nil)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent"])
    }

    @Test
    func aSessionInFlightIsNotAcknowledged() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime()
        book.record(intent: "intent", session: session("intent", stage: .open), progress: nil)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged.isEmpty)
    }

    @Test
    func aFailedAcknowledgementIsTriedAgain() async {
        let book = FundingSessionBook()
        let runtime = StubFundingRuntime(failsFirstAcknowledgement: true)
        book.record(intent: "intent", session: session("intent", stage: delivered(2_000)), progress: nil)

        await book.acknowledgeShown(intent: "intent", runtime: runtime)
        await book.acknowledgeShown(intent: "intent", runtime: runtime)

        #expect(runtime.acknowledged == ["intent", "intent"])
    }
}

/// A provider closing its frame answers the frame `.closed` and hands over to
/// the session sheet.
@MainActor
struct FundingCloseFrameTests {
    @Test
    func closingTheFrameAnswersClosedAndShowsTheSessionSheet() async throws {
        let environment = WindowEnvironment()
        let book = FundingSessionBook()
        book.record(intent: "intent", session: session("intent", stage: .open), progress: nil)
        let coordinator = FundingOverlayCoordinator(
            runtime: StubFundingRuntime(),
            environment: environment,
            book: book
        )

        let answer = Task { await coordinator.presentFrame(providerId: "provider", intent: "intent", route: "/") }
        try await environment.waitUntil { $0.presented.last is FundingProviderFrameController }

        coordinator.closeFrame(providerId: "provider", intent: "intent")

        #expect(await answer.value == .closed)
        try await environment.waitUntil { $0.presented.last is FundingSheetController }
    }

    @Test
    func anotherProvidersCloseLeavesTheFrameOpen() async throws {
        let environment = WindowEnvironment()
        let book = FundingSessionBook()
        book.record(intent: "intent", session: session("intent", stage: .open), progress: nil)
        let coordinator = FundingOverlayCoordinator(
            runtime: StubFundingRuntime(),
            environment: environment,
            book: book
        )

        let answer = Task { await coordinator.presentFrame(providerId: "provider", intent: "intent", route: "/") }
        try await environment.waitUntil { $0.presented.last is FundingProviderFrameController }

        coordinator.closeFrame(providerId: "other", intent: "intent")
        coordinator.closeFrame(providerId: "provider", intent: "elsewhere")
        let frame = try #require(environment.presented.last as? FundingProviderFrameController)
        #expect(frame.presentingViewController != nil)

        coordinator.closeFrame(providerId: "provider", intent: "intent")
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
        providerId: "provider",
        rail: .card,
        asset: "EUR",
        status: .toppedUp,
        payout: nil,
        refunded: nil,
        transactionId: "tx",
        reference: "ref"
    )
}

private func withdrawal(status: FundingStatus, payout: FundingPayout?, refunded: U128?) -> FundingSessionSummary {
    FundingSessionSummary(
        intent: "out",
        ownerProductId: nil,
        direction: .out,
        amount: "50000000",
        stage: .released(debited: "50000000", settledAtMs: openedAtMs + 1_000),
        openedAtMs: openedAtMs,
        providerId: "provider",
        rail: .bank,
        asset: "EUR",
        status: status,
        payout: payout,
        refunded: refunded,
        transactionId: nil,
        reference: nil
    )
}

private func session(_ intent: String, stage: FundingStage) -> FundingSession {
    let quote = FundingQuote(
        quoteId: "quote",
        sendAmount: "50000000",
        receiveAmount: "50000000",
        providerFee: "0",
        networkFee: "0",
        etaSecs: nil,
        expiresAt: nil,
        assetDecimals: 2
    )
    return FundingSession(
        intent: intent,
        ownerProductId: nil,
        direction: .in,
        amount: "50000000",
        stage: stage,
        openedAtMs: openedAtMs,
        deadlineMs: openedAtMs + 86_400_000,
        acknowledged: false,
        providerId: "provider",
        assignedAtMs: openedAtMs,
        cancel: nil,
        updates: [],
        started: [],
        choice: FundingChoice(quote: quote, rail: .card, asset: "EUR", network: nil),
        refund: nil
    )
}

private final class StubFundingRuntime: FundingRuntime, @unchecked Sendable {
    private struct Refused: Error {}

    private let state: OSAllocatedUnfairLock<(failNext: Bool, acknowledged: [String])>
    var acknowledged: [String] { state.withLock { $0.acknowledged } }

    init(failsFirstAcknowledgement: Bool = false) {
        state = OSAllocatedUnfairLock(initialState: (failsFirstAcknowledgement, []))
    }

    func selectFundingOptions(intent _: String, selection _: FundingSelection) {}
    func getFundingQuote(intent _: String, ask _: FundingQuoteAsk) -> UInt64 { 0 }
    func stopFundingQuotes(intent _: String) {}
    func selectFundingProvider(intent _: String, providerId _: String, quoteId _: String?) async throws -> Bool { true }
    func cancelFunding(intent _: String) async throws -> CancelOutcome { .ended }
    func confirmFundingCancel(intent _: String, terms _: CancelTerms?) async throws -> CancelConfirmation { .declined }
    func fundingHistory() -> [FundingSessionSummary] { [] }
    func openFunding(direction _: FundingDirection, amount _: U128?) async throws -> String? { nil }

    func acknowledgeFundingSession(intent: String) async throws -> Bool {
        let failed = state.withLock { state in
            state.acknowledged.append(intent)
            defer { state.failNext = false }
            return state.failNext
        }
        if failed { throw Refused() }
        return true
    }
}

/// Presents for real, over a window of its own, so frames go on and off
/// screen as they do in the app.
@MainActor
private final class WindowEnvironment: FundingOverlayEnvironment {
    private let window: UIWindow
    private(set) var presented: [UIViewController] = []

    init() {
        window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
    }

    var cash: FundingCash { FundingCash(symbol: "CASH", precision: 6) }
    var branding: FundingProviderBranding { PlaceholderBranding() }

    func followSpendable(_: @escaping @MainActor (Decimal) -> Void) async {}

    func present(_ controller: UIViewController) -> Bool {
        guard var top = window.rootViewController else { return false }
        while let next = top.presentedViewController, !next.isBeingDismissed {
            top = next
        }
        top.present(controller, animated: false)
        presented.append(controller)
        return true
    }

    func providerPage(providerId _: String, route _: String, title _: String) -> UIViewController? {
        UIViewController()
    }

    func fundingActivityChanged() {}

    func waitUntil(_ condition: @escaping @MainActor (WindowEnvironment) -> Bool) async throws {
        try await withTimeout(.hangGuard) { @MainActor [self] in
            while !condition(self) {
                try await Task.sleep(for: .milliseconds(20))
            }
        }
    }
}

private struct PlaceholderBranding: FundingProviderBranding {
    func brand(for providerId: String) async -> FundingProviderBrand {
        .placeholder(for: providerId)
    }
}
