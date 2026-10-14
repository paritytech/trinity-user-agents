import Foundation
import PolkadotUI

/// The emoji that the reaction picker of a message offers.
struct ChatReactionChoice {
    let quickEmojis: [String]
    let sections: [EmojiPickerInline.Section]

    /// Every emoji of the host picker.
    static let all = ChatReactionChoice(
        quickEmojis: Chat.quickReactions,
        sections: Chat.allReactions.map { EmojiPickerInline.Section(title: $0.category, emojis: $0.emojis) }
    )

    /// The choice of a room, or nil when the room offers no reactions. A person chat offers every emoji. A
    /// product room offers the emoji that its product allows (`allowed_reactions`), or every emoji.
    static func make(for policy: ChatRoomPolicy?) -> ChatReactionChoice? {
        guard let policy else {
            return .all
        }

        guard policy.messageMenu.builtIns.contains(.react) else {
            return nil
        }

        guard let allowed = policy.messageMenu.allowedReactions else {
            return .all
        }

        guard !allowed.isEmpty else {
            return nil
        }

        let section = EmojiPickerInline.Section(
            title: String(localized: .ChatExtension.reactionsTitle),
            emojis: allowed
        )

        return ChatReactionChoice(
            quickEmojis: Array(allowed.prefix(Chat.quickReactions.count)),
            sections: [section]
        )
    }
}

extension ChatViewModelFactory {
    func addReactionViewModel(
        messageId: String,
        choice: ChatReactionChoice?,
        actions: ChatViewModelActions?
    ) -> ChatMessageContainerConfiguration.AddReactionViewModel? {
        guard let choice else {
            return nil
        }

        return ChatMessageContainerConfiguration.AddReactionViewModel(
            quickEmojis: choice.quickEmojis,
            allSections: choice.sections,
            onReactionTap: actions.map { action in
                { emoji in action.toggleReaction(messageId, emoji) }
            }
        )
    }

    func messageReactionViewModel(
        messageId: String,
        reactions: [ReactionViewModel],
        actions: ChatViewModelActions?
    ) -> ChatMessageContainerConfiguration.MessageReactionViewModel? {
        guard !reactions.isEmpty else {
            return nil
        }

        return ChatMessageContainerConfiguration.MessageReactionViewModel(
            reactions: reactions,
            onReactionTap: actions.map { action in
                { emoji in action.toggleReaction(messageId, emoji) }
            },
            onReactionLongPress: actions.map { action in
                { action.showReactionDetails(messageId) }
            }
        )
    }
}
