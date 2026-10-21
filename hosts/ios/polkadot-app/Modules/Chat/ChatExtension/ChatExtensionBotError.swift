import Foundation
import UIKitExt

enum ChatExtensionBotError: Error {
    /// The extension is listed but its bot was not built in time.
    case notReady(ChatExtension.Id)
}

extension ChatExtensionBotError: ErrorContentConvertible {
    func toErrorContent() -> ErrorContent {
        ErrorContent(
            title: String(localized: .ChatExtension.botNotReadyTitle),
            message: String(localized: .ChatExtension.botNotReadyMessage)
        )
    }
}
