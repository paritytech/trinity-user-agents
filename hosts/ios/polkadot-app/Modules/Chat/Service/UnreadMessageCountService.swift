import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// Counts incoming-new badge messages without materializing chat or message model graphs.
///
/// This service is used by the notification extension, where work happens during a short-lived
/// push-processing window. The fetch reads only `messageId`, `groupingId` and the chat identifier via a
/// dictionary result type to avoid faulting the full message graph through
/// `ChatModelMapper.nonReactionUnreadCount`.
///
/// The count mirrors the app display badge semantics:
/// - incoming `.new` messages are counted.
/// - only messages attached to a chat are considered.
/// - reaction update messages are excluded from the badge count.
/// - system messages are excluded from the badge count.
/// - rows sharing a `groupingId` collapse to a single entry, so e.g. one call (offer + answer +
///   closed) increments the badge by 1.
/// - a product room whose product sets its own badge counts that badge, not its rows. A product-source room has no
///   rows, so its `UnreadCount` badge is the only count it has.
final class UnreadMessageCountService {
    let databaseService: CoreDataServiceProtocol

    init(
        databaseService: CoreDataServiceProtocol = UserDataStorageFacade.shared.databaseService
    ) {
        self.databaseService = databaseService
    }

    func totalUnreadBadgeMessageCount() async throws -> Int {
        try await totalUnreadBadgeMessageCount(unsavedMessageId: nil)
    }

    /// `unsavedMessageId` names a message that was notified but not persisted (a stripped push);
    /// it counts as one more unread entry unless a row for it already exists.
    func totalUnreadBadgeMessageCount(unsavedMessageId: Chat.MessageId?) async throws -> Int {
        try await databaseService.performRead { context in
            let rows = try Self.fetchBadgeRows(in: context)
            let rooms = try Self.fetchBadgeRooms(in: context)
            let savedCount = Self.badgeTotal(rows: rows, rooms: rooms)

            guard let unsavedMessageId, try !Self.messageExists(unsavedMessageId, in: context) else {
                return savedCount
            }

            return savedCount + 1
        }
    }
}

/// A message row that the app badge can count.
struct UnreadBadgeRow: Hashable, Identifiable {
    let messageId: String
    let groupingId: String?
    let chatIdentifier: String?

    var identifier: String {
        messageId
    }
}

/// What the app badge reads of a product room: the badge that its product sets.
struct UnreadBadgeRoom: Equatable, Identifiable {
    let chatIdentifier: String
    /// True when the product sets the badge of the room. Then the rows of the room do not count.
    let setsOwnBadge: Bool
    /// The `UnreadCount` badge of the product. Nil when the product sets no badge.
    let productCount: Int?

    var identifier: String {
        chatIdentifier
    }
}

extension UnreadMessageCountService {
    /// The number of the app badge: one for each grouping id of the rows of the chats that count their rows, and
    /// the `UnreadCount` badges of the product rooms. A room whose product sets the badge counts the badge, not its
    /// rows.
    static func badgeTotal(rows: [UnreadBadgeRow], rooms: [UnreadBadgeRoom]) -> Int {
        let ownBadge = Set(rooms.filter(\.setsOwnBadge).map(\.chatIdentifier))
        let counted = rows.filter { row in
            guard let chat = row.chatIdentifier else { return true }
            return !ownBadge.contains(chat)
        }
        let groups = Set(counted.map { $0.groupingId ?? $0.messageId })
        let productCount = rooms.reduce(0) { $0 + max($1.productCount ?? 0, 0) }

        return groups.count + productCount
    }

    static func badgeCountPredicate() -> NSPredicate {
        NSCompoundPredicate(andPredicateWithSubpredicates: [
            .byStatus(.incoming(.new)),
            NSPredicate(format: "%K != nil", #keyPath(CDChatMessage.chat)),
            NSPredicate(format: "NOT (%K IN %@)", #keyPath(CDChatMessage.contentType), badgeExcludedContentTypes)
        ])
    }

    /// The product rooms that change the app badge: a room whose product sets the badge.
    static func badgeRoomPredicate() -> NSPredicate {
        NSPredicate(
            format: "%K != nil AND %K != %d",
            #keyPath(CDChatRoomMetadata.chat),
            #keyPath(CDChatRoomMetadata.badgeKind),
            hostCountBadgeKind
        )
    }

    static func badgeRoom(of entity: CDChatRoomMetadata) -> UnreadBadgeRoom? {
        guard let chatIdentifier = entity.chat?.identifier else {
            return nil
        }

        return UnreadBadgeRoom(
            chatIdentifier: chatIdentifier,
            setsOwnBadge: entity.badgeKind != hostCountBadgeKind,
            productCount: entity.badgeKind == unreadCountBadgeKind ? Int(entity.badgeCount) : nil
        )
    }
}

private extension UnreadMessageCountService {
    /// The stored kinds of `Chat.RoomBadge` (see `ChatRoomDeclarationMappers.swift`): the host counts the rows, or
    /// the product gives the count.
    static let hostCountBadgeKind: Int16 = 0
    static let unreadCountBadgeKind: Int16 = 2

    static func fetchBadgeRows(in context: NSManagedObjectContext) throws -> [UnreadBadgeRow] {
        let request = NSFetchRequest<NSDictionary>()
        request.entity = CDChatMessage.entity()
        request.predicate = badgeCountPredicate()
        request.resultType = .dictionaryResultType
        request.propertiesToFetch = [
            #keyPath(CDChatMessage.messageId),
            #keyPath(CDChatMessage.groupingId),
            #keyPath(CDChatMessage.chat.identifier)
        ]

        return try context.fetch(request).map { row in
            UnreadBadgeRow(
                messageId: row[#keyPath(CDChatMessage.messageId)] as? String ?? "",
                groupingId: row[#keyPath(CDChatMessage.groupingId)] as? String,
                chatIdentifier: row[#keyPath(CDChatMessage.chat.identifier)] as? String
            )
        }
    }

    static func fetchBadgeRooms(in context: NSManagedObjectContext) throws -> [UnreadBadgeRoom] {
        let request: NSFetchRequest<CDChatRoomMetadata> = CDChatRoomMetadata.fetchRequest()
        request.predicate = badgeRoomPredicate()
        request.relationshipKeyPathsForPrefetching = [#keyPath(CDChatRoomMetadata.chat)]
        return try context.fetch(request).compactMap(badgeRoom(of:))
    }

    static func messageExists(_ messageId: Chat.MessageId, in context: NSManagedObjectContext) throws -> Bool {
        let request = NSFetchRequest<NSNumber>()
        request.entity = CDChatMessage.entity()
        request.predicate = NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.messageId), messageId)
        request.resultType = .countResultType
        return try context.count(for: request) > 0
    }
}

private extension UnreadMessageCountService {
    // Keep this aligned with `Chat.LocalMessage.Content.ContentType.isReaction` and `.isSystem`.
    static let badgeExcludedContentTypes = [
        NSNumber(value: Int16(Chat.LocalMessage.Content.ContentType.reacted.rawValue)),
        NSNumber(value: Int16(Chat.LocalMessage.Content.ContentType.reactionRemoved.rawValue)),
        NSNumber(value: Int16(Chat.LocalMessage.Content.ContentType.token.rawValue)),
        NSNumber(value: Int16(Chat.LocalMessage.Content.ContentType.deviceAdded.rawValue)),
        NSNumber(value: Int16(Chat.LocalMessage.Content.ContentType.deviceRemoved.rawValue))
    ]
}
