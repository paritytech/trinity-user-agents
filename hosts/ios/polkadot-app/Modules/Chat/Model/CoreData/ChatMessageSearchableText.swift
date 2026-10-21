import Foundation

/// The text of a stored message that search matches (`CDChatMessage.searchableText`, 07-LATER §8.2).
///
/// The message mappers write the normalized text of every message: case, diacritics and character width are
/// folded, so a plain `CONTAINS` on the normalized query matches without case and diacritics. A kind with no text
/// gets the empty string. Nil marks a row that a build before search wrote; ``ChatSearchService`` fills it.
enum ChatSearchText {
    static func normalized(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive], locale: nil)
    }

    /// The value of `searchableText` for a message content.
    static func stored(for content: Chat.LocalMessage.Content) -> String {
        content.searchSource.map(normalized) ?? ""
    }
}

extension Chat.LocalMessage.Content {
    /// The text that the user reads in the message, before normalization. An edit gives its new text, so search
    /// finds a message by the text of its edits too. Nil for a kind with no text.
    var searchSource: String? {
        switch self {
        case let .text(text): text
        case let .richText(richText): richText.text
        case let .reply(reply): reply.ownContent.text
        case let .edited(edit): edit.newContent.text
        case let .staticTextImageContent(content): content.text
        case let .file(file): file.text ?? file.name
        case let .productBody(body): body.searchSource
        case let .customRendered(content): content.alt
        case let .extensionActionResponse(text, _): text
        case let .chatRequest(request): request.welcomeMessage?.text
        case let .versionedChatRequest(request): request.ensureV1().welcomeMessage?.text
        case .token,
             .send,
             .coinageSend,
             .contactAdded,
             .leftChat,
             .reacted,
             .reactionRemoved,
             .chatAccepted,
             .multiChatAccepted,
             .unsupported,
             .compactedMessages,
             .deviceAdded,
             .deviceRemoved,
             .call,
             .deleted:
            nil
        }
    }
}

private extension Chat.LocalMessage.Content.ProductBody {
    var searchSource: String? {
        switch self {
        case let .file(file): file.text ?? file.fileName
        case .richText,
             .actions,
             .notice:
            text
        }
    }
}
