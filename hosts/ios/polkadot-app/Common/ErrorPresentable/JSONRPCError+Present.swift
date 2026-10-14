import Foundation
import SubstrateSdk
import UIKitExt

extension JSONRPCError: @retroactive ErrorContentConvertible {
    public func toErrorContent() -> ErrorContent {
        let title: String
        let details: String

        if let data {
            title = message
            details = "\(data) (code \(code))"
        } else {
            title = String(localized: .Common.errorRpc)
            details = "\(message) (code \(code))"
        }

        return ErrorContent(title: title, message: details)
    }
}

extension JSONRPCError: @retroactive LocalizedError {
    /// `data` carries the node's human-readable explanation; `message` is the terse RPC reason.
    public var errorDescription: String? {
        data ?? message
    }
}
