import Foundation
import SubstrateSdk
import UIKitExt

extension Substrate.DispatchCallError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        let title = String(localized: .Common.extrinsicFailedTitle)
        let reason =
            switch self {
            case let .module(moduleError):
                "\(moduleError.display.moduleName): \(moduleError.display.errorName)"
            case let .other(otherError):
                "\(otherError.module): \(otherError.reason ?? "Unknown reason")"
            }

        let message = String(localized: .Common.extrinsicSubmissionMonitorError(reason: reason))

        return ErrorContent(title: title, message: message)
    }
}
