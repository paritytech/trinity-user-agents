import Foundation
import TrUAPIHost

/// One ended funding session as the host keeps it, once the core has handed
/// it over and dropped it.
struct FundingRecord: Identifiable, Equatable {
    enum Outcome: Equatable {
        case delivered
        case released
        case refunded
        case failed(FundingFailure)
    }

    enum Payout: Equatable {
        case paidOut
        case failed(reason: String)
    }

    let intent: String
    let direction: FundingDirection
    let rail: FundingRail?
    let asset: String?
    let providerId: String?
    let requestedAmount: U128?
    let settledAmount: U128?
    /// What the user paid for value in, or receives for value out, in
    /// `paidAsset`, from the quote the session ran on. Kept so the detail
    /// screen can show it once the core has dropped the quote.
    let paidAmount: U128?
    let paidAsset: String?
    /// The crypto network the value moved on: the one the provider named for
    /// the deposit, or the one the quote was asked for.
    let paidNetwork: String?
    let outcome: Outcome
    let payout: Payout?
    let transactionId: String?
    let reference: String?
    let openedAt: Date
    let settledAt: Date

    var id: String { intent }
}

extension FundingRecord {
    /// The record for a session that has ended, or nil while it is open.
    init?(session: FundingSession, progress: FundingProgress?) {
        let settled: Settled
        switch session.stage {
        case .open:
            return nil
        case let .delivered(credited, settledAtMs):
            settled = Settled(outcome: .delivered, amount: credited, atMs: settledAtMs)
        case let .released(debited, settledAtMs):
            settled = Settled(outcome: .released, amount: debited, atMs: settledAtMs)
        case let .failed(reason, settledAtMs):
            let outcome: Outcome = reason == .refunded ? .refunded : .failed(reason)
            settled = Settled(outcome: outcome, amount: nil, atMs: settledAtMs)
        }

        intent = session.intent
        direction = session.direction
        rail = session.choice?.rail
        asset = session.choice?.asset
        providerId = session.providerId
        requestedAmount = session.amount
        settledAmount = settled.amount
        paidAmount = session.choice.map { choice in
            session.direction == .in ? choice.quote.sendAmount : choice.quote.receiveAmount
        }
        paidAsset = session.choice?.asset
        paidNetwork = progress?.deposit.flatMap(\.network) ?? session.choice?.network
        outcome = settled.outcome
        payout = progress?.payout.map(Payout.init)
        transactionId = progress?.transactionId
        reference = progress?.reference
        openedAt = Date(milliseconds: session.openedAtMs)
        settledAt = Date(milliseconds: settled.atMs)
    }

    /// Outbound value is only done once the provider says it paid out; until
    /// then, or for a day, the core keeps the session so a payout can arrive.
    func isFinal(now: Date = Date()) -> Bool {
        guard direction == .out, outcome == .released, payout == nil else { return true }
        return now.timeIntervalSince(settledAt) >= Self.payoutWait
    }

    static let payoutWait: TimeInterval = 24 * 60 * 60
}

private struct Settled {
    let outcome: FundingRecord.Outcome
    let amount: U128?
    let atMs: UInt64
}

private extension FundingDeposit {
    var network: String? {
        guard case let .crypto(_, network, _, _, _, _, _, _) = self else { return nil }
        return network
    }
}

extension FundingRecord.Payout {
    init(_ payout: FundingPayout) {
        switch payout {
        case .paidOut: self = .paidOut
        case let .failed(reason): self = .failed(reason: reason)
        }
    }
}

extension FundingFailure {
    /// A stable name for the record, so a stored failure reads back the same.
    var code: String {
        switch self {
        case .regionUnavailable: "regionUnavailable"
        case .verificationRequired: "verificationRequired"
        case .verificationRefused: "verificationRefused"
        case .belowMinimum: "belowMinimum"
        case .aboveMaximum: "aboveMaximum"
        case .insufficientBalance: "insufficientBalance"
        case .expired: "expired"
        case .wrongAssetOrChain: "wrongAssetOrChain"
        case .providerTimeout: "providerTimeout"
        case .cancelled: "cancelled"
        case .refunded: "refunded"
        case .declined: "declined"
        case let .other(code, _): code
        }
    }

    /// The failure a stored ``code`` names, with the message an `Other`
    /// failure was stored with.
    init(code: String, message: String?) {
        let named: [FundingFailure] = [
            .regionUnavailable, .verificationRequired, .verificationRefused, .belowMinimum, .aboveMaximum,
            .insufficientBalance, .expired, .wrongAssetOrChain, .providerTimeout, .cancelled, .refunded,
            .declined
        ]
        self = named.first { $0.code == code } ?? .other(code: code, message: message ?? "")
    }

    /// The message an `Other` failure carries.
    var message: String? {
        guard case let .other(_, message) = self else { return nil }
        return message
    }
}

extension Date {
    init(milliseconds: UInt64) {
        self.init(timeIntervalSince1970: TimeInterval(milliseconds) / 1_000)
    }
}

/// Where ended funding sessions are kept once the core lets go of them.
protocol FundingHistoryStoring: Sendable {
    func records() async throws -> [FundingRecord]
    func save(_ record: FundingRecord) async throws
}
