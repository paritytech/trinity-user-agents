import Foundation
import SubstrateSdk

protocol ChatNotificationPayloadBuilding {
    func makePayload(for message: Chat.RemoteMessage) throws -> Chat.NotificationPayload
}

/// Chooses between the full and the stripped push payload by SCALE size. Hex encoding doubles the ciphertext, so
/// the plaintext budget is what keeps the relay's APNs JSON under 4 KB.
final class ChatNotificationPayloadBuilder {
    enum Constants {
        static let maxPlainSize = 1_800
    }

    private let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }
}

extension ChatNotificationPayloadBuilder: ChatNotificationPayloadBuilding {
    func makePayload(for message: Chat.RemoteMessage) throws -> Chat.NotificationPayload {
        let full = try Chat.NotificationPayload(full: message)
        let fullSize = try full.scaleEncoded().count

        guard fullSize > Constants.maxPlainSize else {
            logger.debug("Prepared full payload for notification")
            return full
        }

        logger.debug("Payload is to big \(fullSize). Stripping...")

        guard let stripped = Chat.NotificationPayload(stripped: message) else {
            logger.warning(
                "Push payload of \(fullSize) B exceeds \(Constants.maxPlainSize) B and has no stripped form: \(message.messageId)"
            )
            return full
        }

        let strippedSize = try stripped.scaleEncoded().count

        logger.debug("Payload is stripped to \(strippedSize) B")

        if strippedSize > Constants.maxPlainSize {
            logger.warning(
                "Stripped push payload of \(strippedSize) B still exceeds \(Constants.maxPlainSize) B: \(message.messageId)"
            )
        }

        return stripped
    }
}
