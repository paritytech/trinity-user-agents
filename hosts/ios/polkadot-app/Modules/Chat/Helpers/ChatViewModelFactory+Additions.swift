import Foundation

/// "Read aloud" in the room screen (07-LATER §9.5).
extension ChatViewModelFactory {
    /// The host entries of a text message, with "Read aloud" before Delete when the room declares `read_aloud`.
    func withReadAloud(_ actions: [ChatMessageBubbleAction]) -> [ChatMessageBubbleAction] {
        guard MainActor.assumeIsolated({ productRoom?.offersReadAloud == true }) else {
            return actions
        }

        var result = actions
        result.insert(.readAloud, at: result.firstIndex(of: .delete) ?? result.endIndex)
        return result
    }
}

extension Chat.LocalMessage.Content {
    /// The text that "Read aloud" speaks for the message.
    var readAloudText: String {
        switch self {
        case let .text(text): text
        case let .reply(reply): reply.ownContent.text ?? ""
        case let .richText(richText): richText.text ?? String(localized: .Common.mediaFile)
        case let .productBody(body): body.replyPreview
        case let .extensionActionResponse(text, _): text
        default: String(localized: .chatReplyPlaceholder)
        }
    }
}
