import DesignSystem
import FoundationExt
import PolkadotUI
import UIKit

struct ChatRequestConfirmationViewModel {
    let title: String
    let message: NSAttributedString
    let confirmTitle: String
    let keepTitle: String
}

extension ChatRequestConfirmationViewModel {
    static func cancel(username: String?) -> Self {
        .init(
            title: String(localized: .cancelChatRequestTitle),
            message: makeMessage(
                username: username,
                format: { String(localized: .cancelChatRequestMessage(username: $0)) },
                unnamed: String(localized: .cancelChatRequestMessageUnnamed)
            ),
            confirmTitle: String(localized: .cancelChatRequestConfirm),
            keepTitle: String(localized: .cancelChatRequestKeep)
        )
    }

    static func decline(username: String?) -> Self {
        .init(
            title: String(localized: .declineChatRequestTitle),
            message: makeMessage(
                username: username,
                format: { String(localized: .declineChatRequestMessage(username: $0)) },
                unnamed: String(localized: .declineChatRequestMessageUnnamed)
            ),
            confirmTitle: String(localized: .declineChatRequestConfirm),
            keepTitle: String(localized: .declineChatRequestKeep)
        )
    }
}

private extension ChatRequestConfirmationViewModel {
    static func makeMessage(
        username: String?,
        format: (String) -> String,
        unnamed: String
    ) -> NSAttributedString {
        let defaultAttributes = LabelStyle.body16Regular().attributes(
            for: .center,
            lineBreakMode: .byWordWrapping,
            textColor: .fgSecondary
        )

        guard let username else {
            return NSAttributedString(string: unnamed, attributes: defaultAttributes)
        }

        let highlightingAttributes = LabelStyle.title16SemiBold().attributes(
            for: .center,
            lineBreakMode: .byWordWrapping,
            textColor: .fgSecondary
        )

        return NSAttributedString.highlightedItems(
            [username],
            formattingClosure: { items in format(items[0]) },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )
    }
}
