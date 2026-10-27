import Foundation
import TrUAPIHost

// Maps the Chat Extension v2 values of the bindings to the Swift domain values of the chat store, and back
// where the core reads a value. The chat store and the notification extension do not link the bindings.

extension Chat.RoomConfig {
    init(_ config: ChatRoomConfig) {
        self.init(
            productSource: config.productSource,
            reactionDetails: config.reactionDetails,
            reportSeen: config.reportSeen
        )
    }

    var wireValue: ChatRoomConfig {
        ChatRoomConfig(productSource: productSource, reactionDetails: reactionDetails, reportSeen: reportSeen)
    }
}

extension Chat.RoomPreview {
    /// The preview with its last message. The status of a message of the contact says
    /// whether the user saw it.
    init(_ preview: ChatRoomPreview) {
        let badge = preview.badge.map(Chat.RoomBadge.init) ?? .hostDefault

        self.init(
            timestamp: preview.timestamp.map { Int64(clamping: $0) },
            badge: badge,
            lastMessage: preview.lastMessage?.previewMessage(),
            // The native chat list draws the heart for a reaction that the user did not see, and does not count it
            // in the unread badge.
            unseenReaction: preview.unseenReactions > 0
        )
    }
}

extension Chat.RoomBadge {
    init(_ badge: ChatRoomBadge) {
        switch badge {
        case .hidden: self = .none
        case let .unreadCount(count): self = .unreadCount(Int32(clamping: count))
        }
    }
}
