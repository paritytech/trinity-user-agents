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
}

/// Keeps the meta of messages in `CDChatMessageMeta` (07-LATER §9.6). The row belongs to the chat, so a removed
/// room removes its meta.
///
/// In a room that the host stores, the meta of a message needs the message, and the row has the time of the
/// message. In a room that the product keeps, the host has no message to check: the row has the time when the
/// product set the meta, and the store keeps the newest `keptMax` rows of the room.
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
            let timestamp = productKeepsMessages
                ? setAt
                : try Self.requireLiveMessage(productMessageId, chat: chat, chatId: chatId, in: context).timestamp

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
}

private extension ChatMessageMetaStore {
    static func identifier(chatId: Chat.Id, productMessageId: String) -> String {
        "\(chatId.rawRepresentation)/\(productMessageId)"
    }

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
