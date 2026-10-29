import Foundation
import TrUAPIHost

extension FundingFailure {
    /// Why the session ended, in a few words.
    var reasonText: String {
        switch self {
        case .regionUnavailable: String(localized: .Funding.failureRegionUnavailable)
        case .verificationRequired: String(localized: .Funding.failureVerificationRequired)
        case .verificationRefused: String(localized: .Funding.failureVerificationRefused)
        case .belowMinimum: String(localized: .Funding.failureBelowMinimum)
        case .aboveMaximum: String(localized: .Funding.failureAboveMaximum)
        case .insufficientBalance: String(localized: .Funding.failureInsufficientBalance)
        case .expired: String(localized: .Funding.failureExpired)
        case .wrongAssetOrChain: String(localized: .Funding.failureWrongAssetOrChain)
        case .providerTimeout: String(localized: .Funding.failureProviderTimeout)
        case .cancelled: String(localized: .Funding.failureCancelled)
        case .refunded: String(localized: .Funding.failureRefunded)
        case .declined: String(localized: .Funding.failureDeclined)
        case let .other(_, message):
            message.isEmpty ? String(localized: .Funding.failureUnknown) : message
        }
    }

    /// "Failed" and the reason, as a line under a session.
    var failedText: String {
        String(localized: .Funding.failureFailed(reason: reasonText))
    }
}
