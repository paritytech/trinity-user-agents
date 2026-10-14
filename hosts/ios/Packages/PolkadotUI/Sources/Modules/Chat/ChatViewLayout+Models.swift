import Foundation

public extension ChatViewLayout {
    struct ViewModel {
        let headerConfiguration: ChatHeaderConfiguration
        let chatInputConfiguration: (any ChatInputViewConfigurationProtocol)?
        let initiallyVisibleMessageIdentifier: ItemIdentifierType?
        let firstUnreadMessageIdentifier: ItemIdentifierType?
        let scrollDownConfiguration: ScrollDownButtonConfiguration
        let scrollToReactionConfiguration: ScrollToReactionButtonConfiguration
        let sections: [Section]
        let footerConfiguration: (any HashableContentConfiguration)?

        public init(
            headerConfiguration: ChatHeaderConfiguration,
            chatInputConfiguration: (any ChatInputViewConfigurationProtocol)?,
            initiallyVisibleMessageIdentifier: ItemIdentifierType? = nil,
            firstUnreadMessageIdentifier: ItemIdentifierType? = nil,
            scrollDownConfiguration: ScrollDownButtonConfiguration,
            scrollToReactionConfiguration: ScrollToReactionButtonConfiguration = .init(),
            sections: [Section],
            footerConfiguration: (any HashableContentConfiguration)?
        ) {
            self.headerConfiguration = headerConfiguration
            self.chatInputConfiguration = chatInputConfiguration
            self.scrollDownConfiguration = scrollDownConfiguration
            self.scrollToReactionConfiguration = scrollToReactionConfiguration
            self.sections = sections
            self.initiallyVisibleMessageIdentifier = initiallyVisibleMessageIdentifier
            self.firstUnreadMessageIdentifier = firstUnreadMessageIdentifier
            self.footerConfiguration = footerConfiguration
        }
    }
}

public extension ChatViewLayout.ViewModel {
    struct ScrollDownButtonConfiguration {
        let available: Bool
        let unreadCount: Int

        public init(
            available: Bool = true,
            unreadCount: Int = 0
        ) {
            self.available = available
            self.unreadCount = unreadCount
        }
    }
}

public extension ChatViewLayout.ViewModel {
    struct ScrollToReactionButtonConfiguration {
        let targetMessageId: String?

        public init(targetMessageId: String? = nil) {
            self.targetMessageId = targetMessageId
        }
    }
}

public extension ChatViewLayout {
    struct Section {
        let identifier: String
        let dateText: String
        public let messages: [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>]

        public init(
            identifier: String,
            dateText: String,
            messages: [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>]
        ) {
            self.identifier = identifier
            self.dateText = dateText
            self.messages = messages
        }
    }
}
