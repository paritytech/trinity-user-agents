import Foundation

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
        let isPinned1 = chat1.isPinnedToTop
        let isPinned2 = chat2.isPinnedToTop

        guard isPinned1 == isPinned2 else {
            return isPinned1
        }

        return lastMessageComparator(chat1: chat1, chat2: chat2)
    }

    /// The order of the chat list: the pinned rooms first, then the others, each part by recency.
    ///
    /// The last messages give a local order that the product timestamps cannot join, so one pairwise
    /// comparator cannot sort both. The chats that sort by their last message keep the order of
    /// `lastMessageComparator`. A room with a product timestamp goes in before the first of those chats
    /// whose last message is older than the timestamp.
    static func chatListOrder(_ chats: [Chat.LocalModel]) -> [Chat.LocalModel] {
        let pinned = chats.filter(\.isPinnedToTop)
        let others = chats.filter { !$0.isPinnedToTop }

        return byRecency(pinned) + byRecency(others)
    }
}

private extension ChatsComparator {
    static func byRecency(_ chats: [Chat.LocalModel]) -> [Chat.LocalModel] {
        let byMessage = chats
            .filter { $0.productTimestamp == nil }
            .sorted(by: lastMessageComparator)

        let byProduct = chats
            .compactMap { chat in chat.productTimestamp.map { (chat: chat, timestamp: $0) } }
            .sorted { lhs, rhs in
                guard lhs.timestamp != rhs.timestamp else {
                    return lhs.chat.identifier.localizedCompare(rhs.chat.identifier) == .orderedAscending
                }
                return lhs.timestamp > rhs.timestamp
            }

        var result: [Chat.LocalModel] = []
        result.reserveCapacity(chats.count)

        var messageIndex = 0
        for product in byProduct {
            while messageIndex < byMessage.count,
                  byMessage[messageIndex].message.map({ $0.timestamp > product.timestamp }) ?? false {
                result.append(byMessage[messageIndex])
                messageIndex += 1
            }
            result.append(product.chat)
        }

        result.append(contentsOf: byMessage[messageIndex...])
        return result
    }
}

extension Chat.LocalModel {
    /// A room is pinned when the host placed it or when its product pins it.
    var isPinnedToTop: Bool {
        peer.isPinnedToTop || roomPolicy?.listOrder.isPinned == true
    }

    /// The sort key that the product set, in milliseconds since the Unix epoch.
    var productTimestamp: UInt64? {
        roomPolicy?.listOrder.timestamp.map { UInt64(clamping: $0) }
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
