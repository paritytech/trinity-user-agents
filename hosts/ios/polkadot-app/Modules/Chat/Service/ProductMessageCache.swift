import Foundation
import TrUAPIHost

/// What the host keeps of a product-source room after its room screen closes (02-DESIGN §5.3).
struct ProductRoomCacheEntry: Equatable, Sendable {
    /// The newest page, oldest first.
    let messages: [ChatMessage]
    /// The cursor of the page before the newest page.
    let next: String?
    /// The last message that the user saw.
    let seenMessageId: String?
    let seenTimestamp: UInt64?
    /// The last message that the host sent in a `Seen` action.
    let reportedSeenMessageId: String?
}

protocol ProductMessageCaching: Sendable {
    func entry(productId: String, roomId: String) async -> ProductRoomCacheEntry?
    func store(_ entry: ProductRoomCacheEntry, productId: String, roomId: String) async

    /// Changes the cached page of a room that no room screen shows, as an open screen changes its window
    /// (02-DESIGN §5.3 step 3). A room with no cached page stays without one.
    func apply(_ update: ChatFeedUpdate, productId: String, roomId: String) async
}

/// Keeps the newest page of the last rooms in memory. The oldest room goes when the cache is full.
actor ProductMessageCache: ProductMessageCaching {
    static let shared = ProductMessageCache()

    private struct Key: Hashable {
        let productId: String
        let roomId: String
    }

    private let capacity: Int
    /// The most messages of one cached page. A new message past it drops the oldest one.
    private let messageLimit: Int
    private var entries: [Key: ProductRoomCacheEntry] = [:]
    /// The keys, the room of the last use at the end.
    private var order: [Key] = []

    init(capacity: Int = 32, messageLimit: Int = Int(ProductMessageSource.Settings().pageSize)) {
        self.capacity = capacity
        self.messageLimit = max(messageLimit, 1)
    }

    func entry(productId: String, roomId: String) -> ProductRoomCacheEntry? {
        let key = Key(productId: productId, roomId: roomId)
        guard let entry = entries[key] else {
            return nil
        }

        touch(key)
        return entry
    }

    func store(_ entry: ProductRoomCacheEntry, productId: String, roomId: String) {
        let key = Key(productId: productId, roomId: roomId)
        entries[key] = entry
        touch(key)

        while order.count > capacity {
            let evicted = order.removeFirst()
            entries[evicted] = nil
        }
    }

    func apply(_ update: ChatFeedUpdate, productId: String, roomId: String) {
        let key = Key(productId: productId, roomId: roomId)
        guard let entry = entries[key] else {
            return
        }

        entries[key] = entry.applying(update, messageLimit: messageLimit)
    }

    private func touch(_ key: Key) {
        order.removeAll { $0 == key }
        order.append(key)
    }
}

extension ProductRoomCacheEntry {
    /// The entry after a feed update, with the rules of the window of an open room (02-DESIGN §3.3.4).
    ///
    /// A new message goes after the last message that is not newer. When the page holds more than
    /// `messageLimit` messages, the oldest messages go, and the page has no cursor: the next open of the room asks
    /// the newest page again and gets the cursor back.
    func applying(_ update: ChatFeedUpdate, messageLimit: Int) -> ProductRoomCacheEntry {
        var messages = messages
        var next = next

        switch update {
        case let .new(message):
            if let index = messages.firstIndex(where: { $0.messageId == message.messageId }) {
                messages[index] = message
            } else {
                let lastEarlier = messages.lastIndex { $0.timestamp <= message.timestamp }
                messages.insert(message, at: lastEarlier.map { $0 + 1 } ?? 0)
            }

            if messages.count > messageLimit {
                messages.removeFirst(messages.count - messageLimit)
                next = nil
            }
        case let .edited(messageId, body, editedAt):
            Self.modify(&messages, messageId) {
                $0.body = body
                $0.editedAt = editedAt
            }
        case let .replaced(message):
            Self.modify(&messages, message.messageId) { $0 = message }
        case let .reactionPlaced(messageId, participantId, emoji):
            Self.modify(&messages, messageId) { $0.adjustReaction(emoji, by: participantId, placed: true) }
        case let .reactionRemoved(messageId, participantId, emoji):
            Self.modify(&messages, messageId) { $0.adjustReaction(emoji, by: participantId, placed: false) }
        case let .statusChanged(messageId, status):
            Self.modify(&messages, messageId) { $0.status = status }
        }

        return ProductRoomCacheEntry(
            messages: messages,
            next: next,
            seenMessageId: seenMessageId,
            seenTimestamp: seenTimestamp,
            reportedSeenMessageId: reportedSeenMessageId
        )
    }

    private static func modify(
        _ messages: inout [ChatMessage],
        _ messageId: String,
        change: (inout ChatMessage) -> Void
    ) {
        guard let index = messages.firstIndex(where: { $0.messageId == messageId }) else {
            return
        }

        change(&messages[index])
    }
}
