import Foundation
import struct TrUAPIHost.ProductId

enum ChatsComparator {
    static func lastMessageComparator(
        chat1: Chat.LocalModel,
        chat2: Chat.LocalModel
    ) -> Bool {
        guard let message1 = chat1.message, let message2 = chat2.message else {
            return chat1.message != nil ? true : false
        }

        let pair1 = (message1.order, message1.timestamp)
        let pair2 = (message2.order, message2.timestamp)

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

extension Chat.Peer {
    /// A host-placed room is pinned because the host put it there, so it does not compete on
    /// recency. Only the placed room: another room of the same product competes like any chat.
    /// A product bot's extension id is its product id.
    ///
    /// Not gated on the host-placement switch: with it off there is no placed room to pin anyway,
    /// so reading it would only make sorting depend on a singleton.
    var isPinnedToTop: Bool {
        if case let .chatExtension(extensionId, roomId) = self,
           let productId = try? ProductId(extensionId) {
            return HostPlacedProducts.placesRoom(roomId, of: productId)
        }
        return false
    }
}
