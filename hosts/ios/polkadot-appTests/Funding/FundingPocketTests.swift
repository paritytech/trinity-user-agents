import Foundation
@testable import polkadot_app
import Testing
import TrUAPIHost

struct FundingPocketTests {
    private let cash = FundingCash(symbol: "CASH", precision: 2)
    private let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.locale = Locale(identifier: "en_US")
        calendar.timeZone = TimeZone(identifier: "UTC") ?? .current
        return calendar
    }()

    /// Sunday 18 October 2026, 12:00 UTC.
    private var now: Date { Date(timeIntervalSince1970: 1_792_324_800) }

    @Test
    func rowsTitleTheAmountSignedOnceValueMoved() {
        let toppedUp = content(.toppedUp, direction: .in, rail: .card)
        let sent = content(.sent, direction: .out, rail: .crypto)
        let payoutFailed = content(.payoutFailed(refunded: "5000"), direction: .out, rail: .bank)
        let failed = content(.failed(.declined), direction: .in, rail: .card)

        #expect(toppedUp.title == "+$50 CASH")
        #expect(toppedUp.titleTone == .success)
        #expect(toppedUp.icon == .received)
        #expect(toppedUp.subtitle.hasPrefix("Today at "))
        #expect(toppedUp.pill == "Card")

        #expect(sent.title == "-$50 CASH")
        #expect(sent.titleTone == .primary)
        #expect(sent.icon == .sent)

        #expect(payoutFailed.title == "$50 CASH")
        #expect(payoutFailed.titleTone == .secondary)
        #expect(payoutFailed.icon == .failed)
        #expect(payoutFailed.subtitle.hasPrefix("Payout failed · Today at "))

        #expect(failed.title == "$50 CASH")
        #expect(failed.subtitle.hasPrefix("Payment failed · Today at "))
    }

    @Test
    func inFlightRowsUseTheCoreStatusWording() {
        let card = content(.inProgress(.waitingForPayment), direction: .in, rail: .card)
        let crypto = content(.inProgress(.waitingForDeposit), direction: .in, rail: .crypto)
        let retrying = content(.inProgress(.retrying), direction: .in, rail: .card)
        let payout = content(.inProgress(.retryingPayout), direction: .out, rail: .bank)

        #expect(card.title == "$50 CASH")
        #expect(card.subtitle == "Waiting for your payment")
        #expect(card.titleTone == .primary)
        #expect(card.icon == .pending(delayed: false))
        #expect(crypto.subtitle == "Waiting for your deposit")
        #expect(retrying.subtitle == "The provider is retrying. Don't pay again")
        #expect(retrying.isSubtitleWarning)
        #expect(retrying.icon == .pending(delayed: true))
        #expect(payout.title == "-$50 CASH")
        #expect(payout.subtitle == "The provider is retrying the payout")
        #expect(payout.pill == "Bank")
    }

    @Test
    func rowMomentsNameTheDay() {
        let day: TimeInterval = 86_400

        #expect(FundingActivityDate.moment(now, now: now, calendar: calendar).hasPrefix("Today at "))
        #expect(FundingActivityDate.moment(now - day, now: now, calendar: calendar).hasPrefix("Yesterday at "))
        #expect(FundingActivityDate.moment(now - 14 * day, now: now, calendar: calendar).hasPrefix("Oct 4 at "))
        #expect(FundingActivityDate.moment(now - 379 * day, now: now, calendar: calendar) == "Oct 4, 2025")
    }

    @Test
    func theLiveLineTurnsToReconnectingAfterAMinute() {
        #expect(FundingLiveLine.text(heardAt: now - 12, now: now) == "Checked with the provider 12s ago")
        #expect(FundingLiveLine.text(heardAt: now - 60, now: now) == "Trying to reconnect")
    }

    @Test
    func theTimelineDotFollowsTheBand() {
        #expect(FundingStepBar.between(.awaitingPayment) == 1)
        #expect(FundingStepBar.between(.checkingPayment) == 2)
        #expect(FundingStepBar.between(.onItsWay) == 1)
        #expect(FundingStepBar.between(.adding) == 2)
        #expect(FundingStepBar.between(.sending) == 2)
        #expect(FundingStepBar.between(nil) == 1)
    }

    @Test
    func bandLinesFollowTheCoreDocs() {
        let adding = FundingWording.band(.adding, direction: .in, rail: .card, asset: nil, amount: "$50 CASH")
        let retrying = FundingWording.band(.retrying, direction: .in, rail: .bank, asset: nil, amount: nil)
        let sent = FundingWording.band(.sent, direction: .out, rail: .card, asset: nil, amount: nil)
        let delayed = FundingWording.band(.delayed, direction: .out, rail: .bank, asset: nil, amount: nil)

        #expect(adding?.text == "Adding $50 CASH to your balance")
        #expect(adding?.isWarning == false)
        #expect(retrying?.text == "The provider is retrying. Don't send the money again.")
        #expect(retrying?.isWarning == true)
        #expect(sent?.text == "Sent to your card")
        #expect(delayed?.text == "Taking longer than usual. It will continue automatically.")
        #expect(FundingWording.band(.cancel, direction: .in, rail: .card, asset: nil, amount: nil) == nil)
    }

    @Test
    func aTopUpCancelledBeforePayingLeavesTheHistory() {
        #expect(record(.failed(.cancelled), settled: nil).isCancelledUnpaid)
        #expect(!record(.failed(.cancelled), settled: "5000").isCancelledUnpaid)
        #expect(!record(.failed(.declined), settled: nil).isCancelledUnpaid)
    }

    @Test
    func theStickyBarShowsOnceTheCardHasGone() {
        let card = PurseScroll.Card(bottom: 20, balance: 424.13)

        #expect(PurseScroll.bar(card: PurseScroll.Card(bottom: 120, balance: 1)) == nil)
        #expect(PurseScroll.bar(card: card) == PurseScroll.Bar(balance: 424.13))
    }

    @Test
    func theClearingShareNeverDisappearsWhileAnythingClears() {
        #expect(CoinageSplitBar.share(clearing: 0, total: 100) == 0)
        #expect(CoinageSplitBar.share(clearing: 1, total: 100) == 0.06)
        #expect(CoinageSplitBar.share(clearing: 50, total: 100) == 0.5)
        #expect(CoinageSplitBar.share(clearing: 5, total: 0) == 0)
    }
}

private extension FundingPocketTests {
    func item(
        _ status: FundingActivityItem.Status,
        direction: FundingDirection,
        rail: FundingRail? = .card
    ) -> FundingActivityItem {
        FundingActivityItem(
            id: "fs_1",
            direction: direction,
            rail: rail,
            amount: "5000",
            date: now,
            status: status
        )
    }

    func content(
        _ status: FundingActivityItem.Status,
        direction: FundingDirection,
        rail: FundingRail
    ) -> FundingActivityRowContent {
        FundingActivityRowContent(
            item: item(status, direction: direction, rail: rail),
            cash: cash,
            now: now,
            calendar: calendar
        )
    }

    func record(_ outcome: FundingRecord.Outcome, settled: String?) -> FundingRecord {
        FundingRecord(
            intent: "fs_1",
            direction: .in,
            rail: .bank,
            asset: nil,
            providerId: nil,
            requestedAmount: "5000",
            settledAmount: settled,
            paidAmount: nil,
            paidAsset: nil,
            paidDecimals: nil,
            paidNetwork: nil,
            outcome: outcome,
            payout: nil,
            refundedAmount: nil,
            transactionId: nil,
            reference: nil,
            openedAt: now,
            settledAt: now
        )
    }
}
