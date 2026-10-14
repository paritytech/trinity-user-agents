import Foundation
import ExtrinsicService
import UIKitExt

extension FinalExtrinsicStatusError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        let reason =
            switch self {
            case .finalityTimeout:
                "finality timeout"
            case .invalid:
                "invalid"
            case .dropped:
                "dropped"
            case .usurped:
                "replaced by another transaction"
            }

        return ErrorContent(
            title: String(localized: .Common.extrinsicFailedTitle),
            message: String(localized: .Common.extrinsicSubmissionMonitorError(reason: reason))
        )
    }
}
