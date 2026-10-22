import Foundation

enum ChatsComparator {
    static func lastMessageComparator(
        chat1: Chat.LocalModel,
        chat2: Chat.LocalModel
    ) -> Bool {
        guard let message1 = chat1.message, let message2 = chat2.message else {
            return chat1.message != nil ? true : false
        }

        // The preview of a room whose product keeps the messages has no local order, so such a row compares by
        // time. Any other two rows compare by order first.
        let byTime = chat1.keepsProductMessages || chat2.keepsProductMessages
        let pair1 = (byTime ? 0 : message1.order, message1.timestamp)
        let pair2 = (byTime ? 0 : message2.order, message2.timestamp)

        guard pair1 != pair2 else {
            return chat1.identifier.localizedCompare(chat2.identifier) == .orderedAscending
        }

        return pair1 > pair2
    }

    static func lastMessageAndPinnedComparator(
        chat1: Chat.LocalModel,
        chat2: Chat.LocalModel
    ) -> Bool {
        let isPinned1 = chat1.peer.isPinnedToTop
        let isPinned2 = chat2.peer.isPinnedToTop

        guard isPinned1 == isPinned2 else {
            return isPinned1
        }

        return lastMessageComparator(chat1: chat1, chat2: chat2)
    }
}

private extension Chat.LocalModel {
    var keepsProductMessages: Bool {
        roomDeclarations?.effectiveConfig.productSource == true
    }
}

extension Chat.Peer {
    /// A host-placed bot is pinned because the host put it there, so it does not compete on
    /// recency. A product bot's extension id is its product id.
    ///
    /// Not gated on the host-placement switch: with it off there is no placed bot to pin anyway,
    /// so reading it would only make sorting depend on a singleton.
    var isPinnedToTop: Bool {
        if case let .chatExtension(extensionId, _) = self {
            return HostPlacedProducts.contains(productId: extensionId)
        }
        return false
    }
}
