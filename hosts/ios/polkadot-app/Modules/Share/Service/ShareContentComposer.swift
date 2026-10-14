import Foundation
import FoundationExt

protocol ShareContentComposing {
    func compose(items: [ShareItem], userMessage: String?) -> Chat.LocalMessage.Content
    func toActivityItems(_ items: [ShareItem]) -> [Any]
}

final class ShareContentComposer: ShareContentComposing {
    func compose(items: [ShareItem], userMessage: String?) -> Chat.LocalMessage.Content {
        .text(makeTextBody(items: items, userMessage: userMessage))
    }

    func toActivityItems(_ items: [ShareItem]) -> [Any] {
        items.map { item -> Any in
            switch item {
            case let .url(url): url
            case let .text(text): text
            }
        }
    }
}

private extension ShareContentComposer {
    func makeTextBody(items: [ShareItem], userMessage: String?) -> String {
        let trimmedUserMessage = userMessage?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nilIfEmpty

        let itemTexts = items.compactMap { item -> String? in
            switch item {
            case let .url(url): url.absoluteString
            case let .text(text): text.nilIfEmpty
            }
        }

        return ([trimmedUserMessage] + itemTexts)
            .compactMap { $0 }
            .joined(separator: "\n")
    }
}
