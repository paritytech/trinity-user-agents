import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

protocol ChatProductFeedStoring: Sendable {
    /// Applies one feed update of a product to a room that the host stores. Throws `ChatRoomError.unknownRoom`
    /// or `Chat.ProductFeedError`.
    func apply(_ update: Chat.ProductFeedUpdate, productId: ChatExtension.Id, roomId: String) async throws
}

/// Writes the feed updates of a product into the chat store, each in one transaction (02-DESIGN §3.3.4, P4.1).
///
/// The store applies the ownership rules of 02-DESIGN §13 item 6: a product cannot write as the user and
/// cannot edit a message of the user, but it can delete any message of its room. The rows go through
/// `ChatMessageEntityMapper`, so the order, the last message of the chat and the unread marks stay correct.
final class ChatProductFeedStore: ChatProductFeedStoring, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let mapper: ChatMessageEntityMapper

    init(
        databaseService: CoreDataServiceProtocol,
        orderAllocator: ChatMessageOrderAllocating = FileChatMessageOrderAllocator.shared
    ) {
        self.databaseService = databaseService
        mapper = ChatMessageEntityMapper(orderAllocator: orderAllocator)
    }

    func apply(_ update: Chat.ProductFeedUpdate, productId: ChatExtension.Id, roomId: String) async throws {
        let chatId = Chat.Id.chatExtension(productId, roomId: roomId)

        try await databaseService.performWrite { [mapper] context in
            guard let chat: CDChat = try context.first(for: .chat(for: chatId.rawRepresentation)) else {
                throw ChatRoomError.unknownRoom(roomId)
            }

            let writer = FeedWriter(
                chat: chat,
                chatId: chatId,
                productOrigin: .chatExtension(productId),
                mapper: mapper,
                context: context
            )
            try writer.apply(update)
        }
    }
}

/// The writes of one feed update, inside one transaction.
private struct FeedWriter {
    let chat: CDChat
    let chatId: Chat.Id
    /// The origin of every message and reaction that a participant of the product writes.
    let productOrigin: Chat.LocalMessage.Origin
    let mapper: ChatMessageEntityMapper
    let context: NSManagedObjectContext

    func apply(_ update: Chat.ProductFeedUpdate) throws {
        switch update {
        case let .new(message):
            try insert(message)
        case let .edited(messageId, content, editedAt):
            try edit(messageId, content: content, editedAt: editedAt)
        case let .deleted(messageId):
            try delete(messageId)
        case let .reactionPlaced(messageId, participantId, emoji):
            try placeReaction(on: messageId, participantId: participantId, emoji: emoji)
        case let .reactionRemoved(messageId, participantId, emoji):
            try removeReaction(from: messageId, participantId: participantId, emoji: emoji)
        case let .statusChanged(messageId, status):
            try changeStatus(of: messageId, to: status)
        }
    }
}

private extension FeedWriter {
    func insert(_ message: Chat.ProductFeedMessage) throws {
        guard message.participantId != Chat.ProductParticipant.user else {
            throw Chat.ProductFeedError.notOwner
        }

        guard try find(message.messageId) == nil else {
            throw Chat.ProductFeedError.duplicateMessage
        }

        let local = try Chat.LocalMessage(
            messageId: Chat.ProductMessageId.stored(message.messageId, in: chatId),
            chatId: chatId,
            origin: productOrigin,
            creationSource: .localDevice,
            status: .incoming(.new),
            timestamp: message.timestamp,
            content: content(message.content, replyingTo: message.replyTo),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let entity = try context.insertNew(CDChatMessage.self)
        try mapper.populate(entity: entity, from: local, using: context)

        if message.participantId != Chat.ProductParticipant.product {
            entity.originParticipantId = message.participantId
        }
    }

    /// A text that replies to a message of the room becomes a reply, so the bubble quotes the message. A reply
    /// to a message that the room does not hold stays a plain text.
    func content(_ content: Chat.LocalMessage.Content, replyingTo replyTo: String?) throws -> Chat.LocalMessage
        .Content {
        guard
            let replyTo,
            case let .text(text) = content,
            let target = try find(replyTo)?.messageId else {
            return content
        }

        return .reply(.init(messageId: target, ownContent: .init(text: text, attachments: nil)))
    }

    /// Replaces the content in place, so every body kind can change and the chat list shows the new text. A
    /// text body also gets one edit row, so the bubble shows "edited".
    func edit(_ messageId: String, content: Chat.LocalMessage.Content, editedAt: UInt64) throws {
        let entity = try requireLiveMessage(messageId)

        let current = try mapper.transform(entity: entity)
        guard current.origin == productOrigin else {
            throw Chat.ProductFeedError.notOwner
        }

        let newContent: Chat.LocalMessage.Content =
            if case let .reply(reply) = current.content, case let .text(text) = content {
                .reply(.init(messageId: reply.messageId, ownContent: .init(text: text, attachments: nil)))
            } else {
                content
            }

        try mapper.populate(entity: entity, from: current.replacingContent(newContent), using: context)

        if let text = newContent.editableText {
            try saveEditRow(for: current.messageId, text: text, editedAt: editedAt)
        }
    }

    /// One edit row for each message, which each edit replaces. A product that streams an answer edits a
    /// message many times, and the store keeps the newest edit only.
    func saveEditRow(for storedId: Chat.MessageId, text: String, editedAt: UInt64) throws {
        let edit = Chat.LocalMessage(
            messageId: storedId + "#edit",
            chatId: chatId,
            origin: productOrigin,
            creationSource: .localDevice,
            status: .incoming(.seen),
            timestamp: editedAt,
            content: .edited(.init(messageId: storedId, newContent: .init(text: text, attachments: nil))),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let entity = try context.first(for: .chatMessage(with: edit.messageId)) ?? context.insertNew(CDChatMessage.self)
        try mapper.populate(entity: entity, from: edit, using: context)
    }

    /// A delete is idempotent, and a product can delete any message of its room. The meta of the message goes
    /// with it.
    func delete(_ messageId: String) throws {
        guard let entity = try find(messageId), let storedId = entity.messageId else {
            throw Chat.ProductFeedError.unknownMessage
        }

        guard !entity.isTombstone else {
            return
        }

        try ChatMessageMetaStore.deleteMeta(productMessageId: messageId, chatId: chatId, in: context)

        try ChatMessageDeletionMapper().populate(
            entity: entity,
            from: Chat.MessageDeletion(messageId: storedId),
            using: context
        )
    }

    func placeReaction(on messageId: String, participantId: String, emoji: String) throws {
        guard participantId != Chat.ProductParticipant.user else {
            throw Chat.ProductFeedError.notOwner
        }

        let entity = try requireLiveMessage(messageId)
        let identifier = reactionIdentifier(entity, participantId: participantId, emoji: emoji)

        let reaction: CDChatMessageReaction = try context.first(for: .reaction(identifier: identifier))
            ?? context.insertNew(CDChatMessageReaction.self)
        reaction.identifier = identifier
        reaction.emoji = emoji
        reaction.originType = productOrigin.rawType
        reaction.originKey = productOrigin.rawKey
        reaction.timestamp = Int64(bitPattern: Date().toChatTimestamp())
        reaction.message = entity
    }

    func removeReaction(from messageId: String, participantId: String, emoji: String) throws {
        guard participantId != Chat.ProductParticipant.user else {
            throw Chat.ProductFeedError.notOwner
        }

        let entity = try requireLiveMessage(messageId)
        let identifier = reactionIdentifier(entity, participantId: participantId, emoji: emoji)

        if let reaction: CDChatMessageReaction = try context.first(for: .reaction(identifier: identifier)) {
            context.delete(reaction)
        }
    }

    /// The status of a message of the user. The host keeps the read state of a message that a participant of
    /// the product wrote, so a status for such a message changes nothing.
    func changeStatus(of messageId: String, to status: Chat.ProductMessageStatus) throws {
        let entity = try requireLiveMessage(messageId)

        guard
            let storedId = entity.messageId,
            let current = Chat.LocalMessage.Status(rawValue: entity.status),
            current.isOutgoing else {
            return
        }

        try ChatMessageStatusUpdateMapper().populate(
            entity: entity,
            from: Chat.ChatMessageStatusUpdate(messageId: storedId, status: .outgoing(status.outgoing)),
            using: context
        )
    }
}

private extension FeedWriter {
    /// The message of this room that a product id names. The id of a pushed message wins over a host id.
    func find(_ productMessageId: String) throws -> CDChatMessage? {
        let candidates = Chat.ProductMessageId.candidates(productMessageId, in: chatId)

        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            .messages(in: chat),
            .messages(withIds: candidates)
        ])

        let found = try context.fetch(request)
        return found.first { $0.messageId == candidates[0] } ?? found.first
    }

    func requireLiveMessage(_ productMessageId: String) throws -> CDChatMessage {
        guard let entity = try find(productMessageId), !entity.isTombstone else {
            throw Chat.ProductFeedError.unknownMessage
        }

        return entity
    }

    /// A reaction row for each participant and emoji. The reaction of the product itself has the identifier
    /// of a reaction of the first version.
    func reactionIdentifier(_ message: CDChatMessage, participantId: String, emoji: String) -> String {
        let base = "\(message.messageId ?? "")_\(emoji)_\(productOrigin.rawType)"
        return participantId == Chat.ProductParticipant.product ? base : base + "_" + participantId
    }
}

private extension CDChatMessage {
    var isTombstone: Bool {
        contentType == Int16(Chat.LocalMessage.Content.ContentType.deleted.rawValue)
    }
}

private extension NSPredicate {
    static func reaction(identifier: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatMessageReaction.identifier), identifier)
    }
}

private extension Chat.LocalMessage.Content {
    /// The text that an edit row carries, for the bodies whose bubble reads the newest edit.
    var editableText: String? {
        switch self {
        case let .text(text): text
        case let .reply(reply): reply.ownContent.text
        default: nil
        }
    }
}

private extension Chat.ProductMessageStatus {
    var outgoing: Chat.LocalMessage.Status.OutgoingStatus {
        switch self {
        case .preparing: .new
        case .sent: .sent
        case .delivered: .delivered
        case .read: .read
        case .failed: .failed
        }
    }
}
