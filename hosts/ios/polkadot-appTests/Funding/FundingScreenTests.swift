import Foundation
@testable import polkadot_app
import Testing
import TrUAPIHost

struct FundingScreenTests {
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
    func theLiveLineCountsUpFromJustNow() {
        #expect(FundingLiveLine.text(heardAt: now - 2, now: now) == "Checked with the provider just now")
        #expect(FundingLiveLine.text(heardAt: now - 12, now: now) == "Checked with the provider 12s ago")
        #expect(FundingLiveLine.text(heardAt: now - 180, now: now) == "Checked with the provider 3 min ago")
        #expect(FundingLiveLine.text(heardAt: now - 7_300, now: now) == "Checked with the provider 2 h ago")
        let at = { (offset: TimeInterval) in UInt64((now + offset).timeIntervalSince1970 * 1_000) }
        let line = { (fact: FundingFact, offset: TimeInterval?) in
            FundingLiveLine.text(detail: FundingDetail(fact: fact, atMs: offset.map(at)), heardAt: now - 30, now: now)
        }
        #expect(line(.confirmations(done: 2, required: 3), -4) == "Confirmation 2 of 3 · last block 4s ago")
        #expect(line(.parts(done: 1, total: 4), nil) == "Part 1 of 4")
        #expect(line(.scheduled, 12) == "About 12s left")
        #expect(line(.scheduled, -1) == "Any moment now")
        #expect(line(.attempt(n: 2, of: 3), 20) == "Attempt 2 of 3 · next try in 20s")
        #expect(line(.checked, -12) == "Checked with the provider 12s ago")
        #expect(line(.other(text: "Swapping PAS for CASH"), -3) == "Swapping PAS for CASH · 3s ago")
        #expect(line(.expectedBy, nil) == "Checked with the provider 30s ago")
        #expect(FundingLiveLine.text(detail: nil, heardAt: nil, now: now) == nil)
    }

    @Test
    func theKeypadEditsADollarAmountOfAtMostTwoDecimals() {
        let apply = { (key: FundingKeypad.Key, text: String) in FundingKeypad.apply(key, to: text) }

        #expect(apply(.digit("5"), "") == "5")
        #expect(apply(.digit("5"), "0") == "5")
        #expect(apply(.point, "") == "0.")
        #expect(apply(.point, "12.") == "12.")
        #expect(apply(.digit("9"), "12.3") == "12.39")
        #expect(apply(.digit("9"), "12.39") == "12.39")
        #expect(apply(.digit("1"), "1234567") == "1234567")
        #expect(FundingKeypad.apply(.digit("1"), to: "12", maximumIntegerDigits: 2) == "12")
        #expect(apply(.delete, "12.3") == "12.")
        #expect(apply(.delete, "") == "")
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
}

private extension FundingScreenTests {
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
}
