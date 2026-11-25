import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

protocol LeaveChatServicing {
    func leaveChat(_ chat: Chat.LocalModel) async throws
}

final class LeaveChatService {
    private let outboxService: ChatOutboxServicing
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let contactRepository: AnyDataProviderRepository<Chat.Contact>
    private let removalCleanupService: ChatRemovalCleanupServicing
    private let databaseService: CoreDataServiceProtocol

    init(
        outboxService: ChatOutboxServicing,
        removalCleanupService: ChatRemovalCleanupServicing,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) {
        self.outboxService = outboxService
        self.removalCleanupService = removalCleanupService
        databaseService = storageFacade.databaseService

        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )

        contactRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatContactMapper())
            )
        )
    }
}

extension LeaveChatService: LeaveChatServicing {
    func leaveChat(_ chat: Chat.LocalModel) async throws {
        switch chat.peer {
        case let .person(contact):
            try await leaveChat(chat, with: contact)
        case let .chatExtension(extensionId, _):
            try await leaveRoom(chat, of: extensionId)
        }
    }
}

private extension LeaveChatService {
    func leaveChat(_ chat: Chat.LocalModel, with contact: Chat.Contact) async throws {
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            outboxService.sendPeerLeftMessage(to: contact) {
                continuation.resume()
            }
        }

        try await chatRepository.saveOperation({ [] }, { [chat.chatId.rawRepresentation] }).asyncExecute()
        try await contactRepository.saveOperation({ [] }, { [contact.identifier] }).asyncExecute()

        try await removalCleanupService.cleanUpAfterRemoval(of: contact.accountId)
    }

    /// A product learns of the removal from its room list, so nothing is sent. Its last chat takes
    /// the product with it, or the worker would boot again only to recreate the room; opening the
    /// product's app installs it again. The chat and the product are deleted in one write, so a
    /// failure cannot leave the product installed without chats.
    func leaveRoom(_ chat: Chat.LocalModel, of extensionId: ChatExtension.Id) async throws {
        let chatId = chat.chatId.rawRepresentation

        try await databaseService.performWrite { context in
            if let room: CDChat = try context.first(for: .chat(for: chatId)) {
                context.delete(room)
            }

            let remainingChats = CDChat.fetchRequest()
            remainingChats.predicate = .chatsForExtension(extensionId)

            guard try context.count(for: remainingChats) == 0 else {
                return
            }

            let productMatch = NSPredicate(format: "%K == %@", #keyPath(CDProduct.identifier), extensionId)
            if let product: CDProduct = try context.first(for: productMatch) {
                context.delete(product)
            }
        }
    }
}
