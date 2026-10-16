import AsyncAlgorithms
import AsyncExtensions
import CoreData
import Foundation
import Operation_iOS

/// Provides the app badge count from persisted incoming-new chat messages and the badges of product rooms.
protocol UnreadMessageCountServicing {
    func totalUnreadBadgeMessageCount() async throws -> Int
    func totalUnreadBadgeMessageCountStream() -> AnyAsyncSequence<Int>
}

extension UnreadMessageCountService: UnreadMessageCountServicing {
    func totalUnreadBadgeMessageCountStream() -> AnyAsyncSequence<Int> {
        let rows = databaseService.subscribeSnapshot(
            mapper: AnyCoreDataMapper(UnreadBadgeRowMapper()),
            filter: Self.badgeCountPredicate()
        )
        let rooms = databaseService.subscribeSnapshot(
            mapper: AnyCoreDataMapper(UnreadBadgeRoomMapper()),
            filter: Self.badgeRoomPredicate()
        )
        let now = now

        return combineLatest(rows, rooms)
            .map { rows, rooms in Self.badgeTotal(rows: rows, rooms: rooms, at: now()) }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}

/// Reads the grouping id and the chat of a message row for the app badge. It writes nothing.
final class UnreadBadgeRowMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = UnreadBadgeRow
    typealias CoreDataEntity = CDChatMessage

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessage.messageId)
    }

    func transform(entity: CDChatMessage) throws -> UnreadBadgeRow {
        guard let messageId = entity.messageId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDChatMessage.messageId))
        }
        return UnreadBadgeRow(
            messageId: messageId,
            groupingId: entity.groupingId,
            chatIdentifier: entity.chat?.identifier
        )
    }

    func populate(entity _: CDChatMessage, from _: UnreadBadgeRow, using _: NSManagedObjectContext) throws {
        throw CoreDataMapperError.unsupported
    }
}

/// Reads the badge and the mute of a product room for the app badge. It writes nothing.
final class UnreadBadgeRoomMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = UnreadBadgeRoom
    typealias CoreDataEntity = CDChatRoomMetadata

    var entityIdentifierFieldName: String {
        #keyPath(CDChatRoomMetadata.chat.identifier)
    }

    func transform(entity: CDChatRoomMetadata) throws -> UnreadBadgeRoom {
        guard let room = UnreadMessageCountService.badgeRoom(of: entity) else {
            throw CoreDataMapperError.unsupported
        }
        return room
    }

    func populate(entity _: CDChatRoomMetadata, from _: UnreadBadgeRoom, using _: NSManagedObjectContext) throws {
        throw CoreDataMapperError.unsupported
    }
}
