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
}

/// Keeps the newest page of the last rooms in memory. The oldest room goes when the cache is full.
actor ProductMessageCache: ProductMessageCaching {
    static let shared = ProductMessageCache()

    private struct Key: Hashable {
        let productId: String
        let roomId: String
    }

    private let capacity: Int
    private var entries: [Key: ProductRoomCacheEntry] = [:]
    /// The keys, the room of the last use at the end.
    private var order: [Key] = []

    init(capacity: Int = 32) {
        self.capacity = capacity
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

    private func touch(_ key: Key) {
        order.removeAll { $0 == key }
        order.append(key)
    }
}
