import CryptoKit
import Foundation
import SubstrateSdk
import SubstrateSdkExt

/// The text of a push of the chat product: the native chat payload (`NotificationMessagePayload`), sealed with
/// ChaCha20-Poly1305 under the key of the contact's route, nonce first. The texts are the native chat's (A.15.5 of the
/// TypeScript port plan); this file is in the app target too, where the extension's string catalog is not.
enum ProductChatPushText {
    /// The text to show, or nil when the message does not open with `key` or carries nothing to show.
    static func body(messageHex: String, key: Data) -> String? {
        guard let sealed = try? messageHex.fromHex(),
              let box = try? ChaChaPoly.SealedBox(combined: sealed),
              let plain = try? ChaChaPoly.open(box, using: SymmetricKey(data: key)),
              let payload = try? Chat.NotificationPayload.fromScaleEncoded(plain)
        else {
            return nil
        }

        switch payload.versioned {
        case let .v1(.full(message)):
            return text(for: message.content)
        case .v1(.stripped(.coinageSend)):
            return nil
        case let .v1(.stripped(stripped)):
            return text(for: stripped.regularContent)
        }
    }
}

private extension ProductChatPushText {
    static func text(for content: ChatRemoteMessageContent?) -> String? {
        switch content {
        case let .text(string):
            string
        case let .reply(reply):
            reply.ownContent.text
        case let .richText(richText):
            richText.text
        case let .reacted(reaction):
            "reacted with \(reaction.emoji)"
        case .chatAccepted, .multiChatAccepted:
            "approved the chat request"
        default:
            nil
        }
    }
}
