import AsyncExtensions
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

protocol ChatMessageMetaStoring: Sendable {
    /// Replaces the meta of one message of a room. Empty meta removes the row. Throws `ChatRoomError.unknownRoom`,
    /// or `Chat.ProductFeedError.unknownMessage` for a message that a room of the host store does not hold.
    func setMeta(_ meta: Chat.MessageMeta, productMessageId: String, chatId: Chat.Id) async throws

    /// The meta of the messages of a room, by product message id.
    func metaIndex(for chatId: Chat.Id) async throws -> Chat.MessageMetaIndex

    /// Gives the meta of the messages of a room now, and then after each change.
    func metaUpdates(for chatId: Chat.Id) -> AnyAsyncSequence<Chat.MessageMetaIndex>
}

/// Keeps the meta of messages in `CDChatMessageMeta` (07-LATER §9.6). The row belongs to the chat, so a removed
/// room removes its meta.
///
/// In a room that the host stores, the meta of a message needs the message, and the row has the time of the
/// message. In a room that the product keeps, the host has no message to check: the row has the time when the
/// product set the meta, and the store keeps the newest `keptMax` rows of the room.
///
/// A reasoning row is not the chat-list preview and is not unread (07-LATER §9.4). So, in a room that the host
/// stores, the reasoning flag also marks the message as seen and gives the preview to the newest other message.
final class ChatMessageMetaStore: ChatMessageMetaStoring, @unchecked Sendable {
    /// `CHAT_META_KEPT_MAX` of the core.
    static let defaultKeptMax = 2_000

    private let databaseService: CoreDataServiceProtocol
    private let keptMax: Int
    private let now: @Sendable () -> Date

    init(
        databaseService: CoreDataServiceProtocol,
        keptMax: Int = ChatMessageMetaStore.defaultKeptMax,
        now: @escaping @Sendable () -> Date = Date.init
    ) {
        self.databaseService = databaseService
        self.keptMax = keptMax
        self.now = now
    }

    func setMeta(_ meta: Chat.MessageMeta, productMessageId: String, chatId: Chat.Id) async throws {
        let setAt = Int64(clamping: now().toChatTimestamp())

        try await databaseService.performWrite { [keptMax] context in
            guard let chat: CDChat = try context.first(for: .chat(for: chatId.rawRepresentation)) else {
                throw ChatRoomError.unknownRoom(chatId.roomId ?? chatId.rawRepresentation)
            }

            let declarations = Chat.RoomDeclarations(entity: chat.roomMetadata)
            let productKeepsMessages = declarations.effectiveConfig.productSource
            let message = productKeepsMessages
                ? nil
                : try Self.requireLiveMessage(productMessageId, chat: chat, chatId: chatId, in: context)
            let timestamp = message?.timestamp ?? setAt

            if let message, meta.reasoning != nil {
                try Self.hideReasoning(message, chat: chat, in: context)
            }

            let identifier = Self.identifier(chatId: chatId, productMessageId: productMessageId)
            let existing: CDChatMessageMeta? = try context.first(for: .messageMeta(identifier: identifier))

            guard !meta.isEmpty else {
                existing.map(context.delete)
                return
            }

            let entity = try existing ?? context.insertNew(CDChatMessageMeta.self)
            entity.identifier = identifier
            entity.chatId = chatId.rawRepresentation
            entity.messageId = productMessageId
            entity.timestamp = timestamp
            entity.meta = try meta.storedBytes()
            entity.chat = chat

            if productKeepsMessages {
                try Self.trim(chatId: chatId, keptMax: keptMax, in: context)
            }
        }
    }

    func metaIndex(for chatId: Chat.Id) async throws -> Chat.MessageMetaIndex {
        try await databaseService.performRead { context in
            let request: NSFetchRequest<CDChatMessageMeta> = CDChatMessageMeta.fetchRequest()
            request.predicate = .messageMeta(chatId: chatId.rawRepresentation)

            let pairs = try context.fetch(request).compactMap { entity -> (String, Chat.MessageMeta)? in
                guard
                    let messageId = entity.messageId,
                    let meta = entity.meta.flatMap(Chat.MessageMeta.decodedIfStored)
                else {
                    return nil
                }
                return (messageId, meta)
            }

            return Chat.MessageMetaIndex(Dictionary(pairs, uniquingKeysWith: { _, newest in newest }))
        }
    }

    func metaUpdates(for chatId: Chat.Id) -> AnyAsyncSequence<Chat.MessageMetaIndex> {
        databaseService
            .subscribeSnapshot(
                mapper: AnyCoreDataMapper(ChatMessageMetaRowMapper()),
                filter: .messageMeta(chatId: chatId.rawRepresentation)
            )
            .map { rows in
                Chat.MessageMetaIndex(Dictionary(
                    rows.map { ($0.messageId, $0.meta) },
                    uniquingKeysWith: { _, newest in newest }
                ))
            }
            .eraseToAnyAsyncSequence()
    }
}

extension ChatMessageMetaStore {
    /// The identifier of the meta row of a message.
    static func identifier(chatId: Chat.Id, productMessageId: String) -> String {
        "\(chatId.rawRepresentation)/\(productMessageId)"
    }

    /// Deletes the meta of a message, because the message is gone (07-LATER §9.6).
    static func deleteMeta(productMessageId: String, chatId: Chat.Id, in context: NSManagedObjectContext) throws {
        let identifier = identifier(chatId: chatId, productMessageId: productMessageId)
        let row: CDChatMessageMeta? = try context.first(for: .messageMeta(identifier: identifier))
        row.map(context.delete)
    }
}

private extension ChatMessageMetaStore {
    /// The message of a room of the host store that a product id names. A tombstone is not a message.
    static func requireLiveMessage(
        _ productMessageId: String,
        chat: CDChat,
        chatId: Chat.Id,
        in context: NSManagedObjectContext
    ) throws -> CDChatMessage {
        let candidates = Chat.ProductMessageId.candidates(productMessageId, in: chatId)
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            .messages(in: chat),
            .messages(withIds: candidates)
        ])

        let found = try context.fetch(request)
        let tombstone = Int16(Chat.LocalMessage.Content.ContentType.deleted.rawValue)
        guard
            let message = found.first(where: { $0.messageId == candidates[0] }) ?? found.first,
            message.contentType != tombstone
        else {
            throw Chat.ProductFeedError.unknownMessage
        }

        return message
    }

    /// The content kinds that the chat list does not show as the preview.
    static let previewExcludedContentTypes: [NSNumber] = (UInt8.min ... UInt8.max)
        .compactMap(Chat.LocalMessage.Content.ContentType.init(rawValue:))
        .filter(\.isExcludedFromChatList)
        .map { NSNumber(value: Int16($0.rawValue)) }

    /// Marks a reasoning message of the product as seen. When the message is the chat-list preview, the newest other
    /// message that the list can show becomes the preview.
    static func hideReasoning(_ message: CDChatMessage, chat: CDChat, in context: NSManagedObjectContext) throws {
        if message.status == Chat.LocalMessage.Status.incoming(.new).rawValue {
            message.status = Chat.LocalMessage.Status.incoming(.seen).rawValue
        }

        for mark in (chat.unreadMessages as? Set<CDChatUnreadMessage>) ?? [] where mark.messageId == message.messageId {
            context.delete(mark)
        }

        guard chat.lastDisplayMessage == message else {
            return
        }

        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            .messages(in: chat),
            NSPredicate(format: "SELF != %@", message),
            NSPredicate(format: "NOT (%K IN %@)", #keyPath(CDChatMessage.contentType), previewExcludedContentTypes)
        ])
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.order), ascending: false),
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)
        ]
        request.fetchLimit = 1

        message.lastDisplayMessageChat = nil
        chat.lastDisplayMessage = try context.fetch(request).first
    }

    /// Removes the oldest rows of a room above `keptMax`.
    static func trim(chatId: Chat.Id, keptMax: Int, in context: NSManagedObjectContext) throws {
        let request: NSFetchRequest<CDChatMessageMeta> = CDChatMessageMeta.fetchRequest()
        request.predicate = .messageMeta(chatId: chatId.rawRepresentation)
        request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDChatMessageMeta.timestamp), ascending: false)]

        // The result also has the rows of this transaction, because the fetch includes changes that are not saved.
        let rows = try context.fetch(request)
        for row in rows.dropFirst(keptMax) {
            context.delete(row)
        }
    }
}

extension NSPredicate {
    static func messageMeta(identifier: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatMessageMeta.identifier), identifier)
    }

    static func messageMeta(chatId: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatMessageMeta.chatId), chatId)
    }
}

/// One stored row of meta, for a subscription to the meta of a room.
struct ChatMessageMetaRow: Identifiable, Equatable {
    let identifier: String
    /// The id under which the product knows the message.
    let messageId: String
    let meta: Chat.MessageMeta
}

final class ChatMessageMetaRowMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = ChatMessageMetaRow
    typealias CoreDataEntity = CDChatMessageMeta

    enum MapperError: Error {
        case unreadable
        case readOnly
    }

    var entityIdentifierFieldName: String {
        #keyPath(CDChatMessageMeta.identifier)
    }

    func transform(entity: CDChatMessageMeta) throws -> ChatMessageMetaRow {
        guard
            let identifier = entity.identifier,
            let messageId = entity.messageId,
            let meta = entity.meta.flatMap(Chat.MessageMeta.decodedIfStored)
        else {
            throw MapperError.unreadable
        }

        return ChatMessageMetaRow(identifier: identifier, messageId: messageId, meta: meta)
    }

    /// The store writes the rows through ``ChatMessageMetaStore``, not through this mapper.
    func populate(entity _: CDChatMessageMeta, from _: ChatMessageMetaRow, using _: NSManagedObjectContext) throws {
        throw MapperError.readOnly
    }
}
