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
    func rowsWordEachEndingAsThePrototypeDoes() {
        let toppedUp = content(.toppedUp, direction: .in, rail: .card)
        let sent = content(.sent, direction: .out, rail: .crypto)
        let payoutFailed = content(.payoutFailed(refunded: "5000"), direction: .out, rail: .bank)
        let failed = content(.failed(.declined), direction: .in, rail: .card)

        #expect(toppedUp.title == "Topped up")
        #expect(toppedUp.amount == "+$50 CASH")
        #expect(toppedUp.amountTone == .success)
        #expect(toppedUp.icon == .received)
        #expect(toppedUp.subtitle.hasPrefix("Card · Today at "))

        #expect(sent.title == "Sent")
        #expect(sent.amount == "-$50 CASH")
        #expect(sent.amountTone == .primary)
        #expect(sent.icon == .sent)

        #expect(payoutFailed.title == "Payout failed")
        #expect(payoutFailed.amount == "$50 CASH")
        #expect(payoutFailed.amountTone == .secondary)
        #expect(payoutFailed.icon == .failed)
        #expect(payoutFailed.subtitle.hasPrefix("Bank · Today at "))

        #expect(failed.title == "Top-up failed")
        #expect(failed.subtitle == "Payment declined")
        #expect(failed.amount == "$50 CASH")
    }

    @Test
    func inFlightRowsUseTheCoreStatusWording() {
        let card = content(.inProgress(.waitingForPayment), direction: .in, rail: .card)
        let crypto = content(.inProgress(.waitingForDeposit), direction: .in, rail: .crypto)
        let retrying = content(.inProgress(.retrying), direction: .in, rail: .card)
        let payout = content(.inProgress(.retryingPayout), direction: .out, rail: .bank)

        #expect(card.title == "Topping up")
        #expect(card.subtitle == "Upcoming receiving")
        #expect(card.amount == "+$50 CASH")
        #expect(card.amountTone == .secondary)
        #expect(card.icon == .pending(delayed: false))
        #expect(crypto.subtitle == "Upcoming receiving")
        #expect(retrying.subtitle == "Provider is retrying your payment")
        #expect(retrying.isSubtitleWarning)
        #expect(retrying.icon == .pending(delayed: true))
        #expect(payout.title == "Sending")
        #expect(payout.subtitle == "Provider is retrying the payout")
        #expect(payout.amount == "-$50 CASH")
    }

    @Test
    func dayHeadingsNameTheWeekdayAndMonth() {
        let day: TimeInterval = 86_400

        #expect(FundingActivityDate.day(now, now: now, calendar: calendar) == "Today")
        #expect(FundingActivityDate.day(now - day, now: now, calendar: calendar) == "Yesterday")
        #expect(FundingActivityDate.day(now - 14 * day, now: now, calendar: calendar) == "Sunday 4 October")
        #expect(FundingActivityDate.day(now - 379 * day, now: now, calendar: calendar) == "Saturday 4 October 2025")
    }

    @Test
    func rowMomentsKeepTheShortDate() {
        let day: TimeInterval = 86_400

        #expect(FundingActivityDate.moment(now - 379 * day, now: now, calendar: calendar) == "4 Oct 2025")
        #expect(FundingActivityDate.moment(now - 14 * day, now: now, calendar: calendar).hasPrefix("4 Oct at "))
    }

    @Test
    func eachDayClosesOnTheBalanceBeforeLaterDays() {
        #expect(
            FundingDayBalances.closing(current: 424.13, dayChanges: [-25, 85.25, 0])
                == [424.13, 449.13, 363.88]
        )
    }

    @Test
    func onlyToppedUpAndSentChangeTheBalance() {
        #expect(item(.toppedUp, direction: .in).balanceChange(cash: cash) == 50)
        #expect(item(.sent, direction: .out).balanceChange(cash: cash) == -50)
        #expect(item(.payoutFailed(refunded: nil), direction: .out).balanceChange(cash: cash) == 0)
        #expect(item(.inProgress(.transactionInitiated), direction: .out).balanceChange(cash: cash) == 0)
    }

    @Test
    func theDockFrontsADelayedSessionThenTheNewest() {
        let older = item(.inProgress(.waitingForPayment), direction: .in, id: "older", date: now - 60)
        let newer = item(.inProgress(.transactionInitiated), direction: .out, id: "newer", date: now)
        let delayed = item(.inProgress(.retrying), direction: .in, id: "delayed", date: now - 600)

        #expect(FundingInProgressDock.ordered([older, newer]).map(\.id) == ["newer", "older"])
        #expect(FundingInProgressDock.ordered([older, newer, delayed]).map(\.id) == ["delayed", "newer", "older"])
    }

    @Test
    func theStickyBarShowsOnceTheCardHasGone() {
        let card = PurseScroll.Card(bottom: 20, balance: 424.13)
        let today = PurseScroll.DayMark(title: "Today", isToday: true, balance: 424.13, top: -200)
        let yesterday = PurseScroll.DayMark(title: "Yesterday", isToday: false, balance: 449.13, top: 30)
        let older = PurseScroll.DayMark(title: "Sunday 4 October", isToday: false, balance: 363.88, top: 400)

        #expect(PurseScroll.bar(card: PurseScroll.Card(bottom: 120, balance: 1), days: [today]) == nil)
        #expect(
            PurseScroll.bar(card: card, days: [today])
                == PurseScroll.Bar(label: .currentBalance, balance: 424.13)
        )
        #expect(
            PurseScroll.bar(card: card, days: [today, yesterday, older])
                == PurseScroll.Bar(label: .day("Yesterday"), balance: 449.13)
        )
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
        rail: FundingRail? = .card,
        id: String = "fs_1",
        date: Date? = nil
    ) -> FundingActivityItem {
        FundingActivityItem(
            id: id,
            direction: direction,
            rail: rail,
            amount: "5000",
            date: date ?? now,
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
}
