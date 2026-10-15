import CoreData
import Foundation
import os
import TrUAPIHost

/// The parent room and the root message of a thread room, for the header and the pinned quote (07-LATER §9.2).
struct ChatThreadRoot: Equatable {
    /// The name of the parent room, or nil when the host does not know the room.
    let parentName: String?
    /// Who wrote the root: "You", or the name of the author. Nil when the root text is not known.
    let author: String?
    /// The text of the root, or nil when the host does not hold the root ("Earlier message").
    let text: String?
}

protocol ChatThreadRootResolving: Sendable {
    /// Keeps the root of a thread that the screen of the parent room shows now, for the thread screen that opens.
    func handOver(_ root: ChatThreadRoot, threadChatId: Chat.Id)

    /// The parent room and the root of a thread room.
    func root(of parent: Chat.ThreadParent, threadChatId: Chat.Id) async -> ChatThreadRoot
}

/// Finds the root of a thread: first in what the screen of the parent room gave, then in the chat store (a room
/// that the host stores), then in the cache of the newest page (a room that the product keeps). Else the root text
/// is not known, and the thread shows "Earlier message".
final class ChatThreadRootResolver: ChatThreadRootResolving, @unchecked Sendable {
    static let shared = ChatThreadRootResolver(
        storageFacade: UserDataStorageFacade.shared,
        pageCache: ProductMessageCache.shared
    )

    private let storageFacade: StorageFacadeProtocol
    private let pageCache: ProductMessageCaching
    private let handedOver = OSAllocatedUnfairLock(initialState: [String: ChatThreadRoot]())

    init(storageFacade: StorageFacadeProtocol, pageCache: ProductMessageCaching) {
        self.storageFacade = storageFacade
        self.pageCache = pageCache
    }

    func handOver(_ root: ChatThreadRoot, threadChatId: Chat.Id) {
        handedOver.withLock { $0[threadChatId.rawRepresentation] = root }
    }

    func root(of parent: Chat.ThreadParent, threadChatId: Chat.Id) async -> ChatThreadRoot {
        if let root = handedOver.withLock({ $0[threadChatId.rawRepresentation] }), root.text != nil {
            return root
        }

        guard case let .chatExtension(productId, _) = threadChatId else {
            return ChatThreadRoot(parentName: nil, author: nil, text: nil)
        }

        let parentChatId = Chat.Id.chatExtension(productId, roomId: parent.roomId)
        let stored = try? await storedRoot(parentChatId: parentChatId, messageId: parent.messageId)

        if let stored, stored.text != nil {
            return stored
        }

        let cached = await pageCache.entry(productId: productId, roomId: parent.roomId)?
            .messages
            .first { $0.messageId == parent.messageId }

        return ChatThreadRoot(
            parentName: stored?.parentName,
            author: cached.map { $0.isFromUser ? String(localized: .chatReplyYou) : stored?.parentName ?? "" },
            text: cached.map(\.body.localContent.threadRootText)
        )
    }
}

private extension ChatThreadRootResolver {
    func storedRoot(parentChatId: Chat.Id, messageId: String) async throws -> ChatThreadRoot {
        try await storageFacade.databaseService.performRead { context in
            let chat: CDChat? = try context.first(for: .chat(for: parentChatId.rawRepresentation))
            let parentName = chat?.roomMetadata?.name

            guard let chat else {
                return ChatThreadRoot(parentName: nil, author: nil, text: nil)
            }

            let candidates = Chat.ProductMessageId.candidates(messageId, in: parentChatId)
            let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
            request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
                .messages(in: chat),
                .messages(withIds: candidates)
            ])
            let found = try context.fetch(request)

            guard
                let message = found.first(where: { $0.messageId == candidates[0] }) ?? found.first,
                let content = try? ChatMessageEntityMapper.getContent(from: message),
                let status = Chat.LocalMessage.Status(rawValue: message.status)
            else {
                return ChatThreadRoot(parentName: parentName, author: nil, text: nil)
            }

            return ChatThreadRoot(
                parentName: parentName,
                author: status.isOutgoing ? String(localized: .chatReplyYou) : parentName,
                text: content.threadRootText
            )
        }
    }
}

extension Chat.LocalMessage.Content {
    /// The text that the pinned quote of a thread shows for its root.
    var threadRootText: String {
        switch self {
        case let .text(text): text
        case let .reply(reply): reply.ownContent.text ?? ""
        case let .richText(richText): richText.text ?? String(localized: .Common.mediaFile)
        case let .productBody(body): body.replyPreview
        case let .extensionActionResponse(text, _): text
        default: String(localized: .chatReplyPlaceholder)
        }
    }
}
