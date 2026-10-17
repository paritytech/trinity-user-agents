import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

protocol ChatRoomRemoving: Sendable {
    /// Removes a product room, its thread rooms and everything that the host keeps for them. Gives false when no
    /// such chat exists.
    func removeRoom(_ chatId: Chat.Id) async throws -> Bool
}

/// Removes a product room in one transaction (02-DESIGN §5.5). The cascade rules of the model remove the
/// messages, their reactions, the unread marks, the meta and the declarations with the chat. The extension
/// history of the room has no relationship to the chat, so the remover deletes it by the chat id. After the
/// transaction the remover evicts the images and the media that the room referenced from the resource caches.
///
/// The chat list does not show a thread room, so the remover also removes each room of the product whose
/// `thread_of` names the room, in the same transaction (07-LATER §9.2).
final class ChatRoomRemover: ChatRoomRemoving, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let resources: ChatResourceEvicting

    init(databaseService: CoreDataServiceProtocol, resources: ChatResourceEvicting) {
        self.databaseService = databaseService
        self.resources = resources
    }

    func removeRoom(_ chatId: Chat.Id) async throws -> Bool {
        let referenced = try await databaseService.performWrite { context in
            try Self.deleteRoom(chatId, in: context)
        }

        guard let referenced else {
            return false
        }

        await resources.evict(referenced)
        return true
    }
}

private extension ChatRoomRemover {
    /// Deletes the room and its thread rooms, and gives the resources that they referenced, or nil when no such
    /// chat exists.
    static func deleteRoom(_ chatId: Chat.Id, in context: NSManagedObjectContext) throws -> [ChatResource]? {
        let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))

        guard let chat else {
            return nil
        }

        var referenced = try deleteChat(chat, chatId: chatId, in: context)
        for thread in try threadChats(of: chatId, in: context) {
            guard let threadId = thread.identifier.flatMap(Chat.Id.fromRawRepresentation) else { continue }
            referenced += try deleteChat(thread, chatId: threadId, in: context)
        }

        return referenced
    }

    /// The rooms of the same product whose features name the room as their parent.
    static func threadChats(of chatId: Chat.Id, in context: NSManagedObjectContext) throws -> [CDChat] {
        guard case let .chatExtension(productId, roomId?) = chatId else {
            return []
        }

        let request: NSFetchRequest<CDChat> = CDChat.fetchRequest()
        request.predicate = .roomChatsForExtension(productId)

        return try context.fetch(request).filter { chat in
            let features = chat.roomMetadata?.features.flatMap(Chat.RoomFeatures.decodedIfStored)
            return features?.threadOf?.roomId == roomId && chat.identifier != chatId.rawRepresentation
        }
    }

    static func deleteChat(_ chat: CDChat, chatId: Chat.Id, in context: NSManagedObjectContext) throws
        -> [ChatResource] {
        let referenced = referencedResources(of: chat)

        let historyRequest: NSFetchRequest<CDChatExtensionHistory> = CDChatExtensionHistory.fetchRequest()
        historyRequest.predicate = .chatExtensionHistoryForChat(chatId)
        historyRequest.includesPropertyValues = false

        for history in try context.fetch(historyRequest) {
            context.delete(history)
        }

        context.delete(chat)

        return referenced
    }

    static func referencedResources(of chat: CDChat) -> [ChatResource] {
        let declarations = Chat.RoomDeclarations(entity: chat.roomMetadata)

        var images = [declarations.avatar].compactMap(\.self)
        if case let .image(badgeImage) = declarations.preview.badge {
            images.append(badgeImage)
        }

        let iconSources = [chat.roomMetadata?.icon].compactMap(\.self)

        return images.map(\.resource) + (iconSources + productMediaSources(of: chat))
            .compactMap(ChatResource.init(source:))
    }

    static func productMediaSources(of chat: CDChat) -> [String] {
        let productBodyType = Int16(Chat.LocalMessage.Content.ContentType.productBody.rawValue)
        let messages = (chat.messages as? Set<CDChatMessage>) ?? []

        return messages
            .filter { $0.contentType == productBodyType }
            .compactMap { try? ChatMessageEntityMapper.getContent(from: $0) }
            .flatMap { content -> [String] in
                guard case let .productBody(body) = content else { return [] }
                return body.resourceSources
            }
    }
}

private extension Chat.LocalMessage.Content.ProductBody {
    var resourceSources: [String] {
        switch self {
        case let .richText(richText): richText.media
        case let .file(file): [file.url]
        case .actions,
             .notice: []
        }
    }
}
