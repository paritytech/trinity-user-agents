import Foundation
import UIKitExt

extension RampAction {
    func errorContent(for error: Error) -> ErrorContent {
        ErrorContent(title: errorTitle, message: errorMessage(for: error))
    }
}

private extension RampAction {
    var errorTitle: String {
        switch self {
        case .topUp: String(localized: .Products.fundingTopUpUnavailableTitle)
        case .withdraw: String(localized: .Products.fundingWithdrawUnavailableTitle)
        }
    }

    func errorMessage(for error: Error) -> String {
        guard let fundingError = error as? FundingDomainError else {
            return String(localized: .Products.fundingErrorUnknown)
        }

        return switch fundingError {
        case .remoteConfigUnavailable: String(localized: .Products.fundingErrorConfigNotLoaded)
        case .destinationNotConfigured: String(localized: .Products.fundingErrorNotConfigured)
        case .networkUnavailable: String(localized: .Products.fundingErrorNetworkUnavailable)
        case .destinationNotOnNetwork: String(localized: .Products.fundingErrorWrongNetwork)
        }
    }
}
