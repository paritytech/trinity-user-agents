import Foundation
import TrUAPIHost

/// How the app words a session, as the core's `FundingStatus` and
/// `FundingBand` docs say each one reads.
enum FundingWording {
    /// The line under a session's timeline, and whether it reads as a
    /// warning. `nil` for a band whose words come from the status instead,
    /// a cancel or an ending.
    static func band(
        _ band: FundingBand,
        direction: FundingDirection,
        rail: FundingRail?,
        asset: String?,
        amount: String?
    ) -> (text: String, isWarning: Bool)? {
        let asset = asset ?? ""
        let amount = amount ?? ""
        switch band {
        case .awaitingPayment:
            return (awaitingText(rail: rail), false)
        case .checkingPayment:
            return rail == .crypto
                ? (String(localized: .Funding.bandConfirmingDeposit(asset: asset)), false)
                : (String(localized: .Funding.bandChecking), false)
        case .onItsWay:
            return (String(localized: .Funding.bandOnItsWay), false)
        case .adding:
            return (String(localized: .Funding.bandAdding(amount: amount)), false)
        case .added:
            return (String(localized: .Funding.bandAdded(amount: amount)), false)
        case .converting:
            return (String(localized: .Funding.bandConverting), false)
        case .preparingPayout:
            return rail == .crypto
                ? (String(localized: .Funding.bandExchanging(asset: asset)), false)
                : (String(localized: .Funding.bandPreparingPayout), false)
        case .sending:
            return rail == .crypto
                ? (String(localized: .Funding.bandSendingAsset(asset: asset)), false)
                : (String(localized: .Funding.bandSendingRail(rail: railWord(rail))), false)
        case .sent:
            return rail == .crypto
                ? (String(localized: .Funding.bandSentAsset(asset: asset)), false)
                : (String(localized: .Funding.bandSentRail(rail: railWord(rail))), false)
        case .retrying:
            if direction == .out { return (String(localized: .Funding.bandRetryingPayout), true) }
            return rail == .bank
                ? (String(localized: .Funding.bandRetryingBank), true)
                : (String(localized: .Funding.bandRetrying), true)
        case .delayed:
            return direction == .out
                ? (String(localized: .Funding.bandDelayedOut), true)
                : (String(localized: .Funding.bandDelayed), true)
        case .cancel,
             .ended:
            return nil
        }
    }

    /// A history row's subtitle for a session in flight.
    static func inFlight(_ status: FundingStatus, direction: FundingDirection) -> String {
        if let line = FundingCancelPresentation.line(for: status, compact: true) {
            return line.text
        }
        return switch status {
        case .waitingForPayment: String(localized: .Funding.activityWaitingPayment)
        case .waitingForTransfer: String(localized: .Funding.activityWaitingTransfer)
        case .waitingForDeposit: String(localized: .Funding.activityWaitingDeposit)
        case .convertingToCash: String(localized: .Funding.activityOnItsWay)
        case .addingToBalance: String(localized: .Funding.activityAdding)
        case .transactionInitiated: String(localized: .Funding.activityInitiated)
        case let .convertingTo(asset): String(localized: .Funding.activityConvertingOut(asset: asset))
        case .preparingPayout: String(localized: .Funding.activityPreparingPayout)
        case .retrying: String(localized: .Funding.activityRetrying)
        case .retryingPayout: String(localized: .Funding.activityRetryingPayout)
        case .takingLonger:
            direction == .out
                ? String(localized: .Funding.activityDelayedOut)
                : String(localized: .Funding.activityDelayed)
        default: ""
        }
    }

    /// A card or bank, as a band line names it.
    static func railWord(_ rail: FundingRail?) -> String {
        rail == .card ? String(localized: .Funding.bandRailCard) : String(localized: .Funding.bandRailBank)
    }

    private static func awaitingText(rail: FundingRail?) -> String {
        switch rail {
        case .bank: String(localized: .Funding.progressBankNote)
        case .crypto: String(localized: .Funding.bandWaitingDeposit)
        case .card,
             nil: String(localized: .Funding.bandWaitingPayment)
        }
    }
}
