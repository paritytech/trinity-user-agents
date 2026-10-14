import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

protocol ChatRoomRemoving: Sendable {
    /// Removes a product room and everything that the host keeps for it. Gives false when no such chat exists.
    func removeRoom(_ chatId: Chat.Id) async throws -> Bool
}

/// Removes a product room in one transaction (02-DESIGN §5.5). The cascade rules of the model remove the
/// messages, their reactions, the unread marks and the declarations with the chat. The extension history of
/// the room has no relationship to the chat, so the remover deletes it by the chat id. After the transaction
/// the remover evicts the images and the media that the room referenced from the resource caches.
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
    /// Deletes the room and gives the resources that it referenced, or nil when no such chat exists.
    static func deleteRoom(_ chatId: Chat.Id, in context: NSManagedObjectContext) throws -> [ChatResource]? {
        let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))

        guard let chat else {
            return nil
        }

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
        case .actions: []
        }
    }
}
