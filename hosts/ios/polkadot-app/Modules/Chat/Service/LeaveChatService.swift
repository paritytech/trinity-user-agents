import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import Products
import SubstrateSdk

protocol LeaveChatServicing {
    func leaveChat(_ chat: Chat.LocalModel) async throws
}

final class LeaveChatService {
    private let outboxService: ChatOutboxServicing
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let contactRepository: AnyDataProviderRepository<Chat.Contact>
    private let removedChatRepository: AnyDataProviderRepository<Chat.RemovedChat>
    private let productRepository: AnyDataProviderRepository<Product>
    private let messageExchangeModeProvider: MessageExchangeModeProviding
    private let storageFacade: StorageFacadeProtocol

    init(
        outboxService: ChatOutboxServicing,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        productRepository: AnyDataProviderRepository<Product>,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) {
        self.outboxService = outboxService
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.productRepository = productRepository
        self.storageFacade = storageFacade

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

        removedChatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(RemovedChatMapper())
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

        try await deleteChat(chat)
        try await contactRepository.saveOperation({ [] }, { [contact.identifier] }).asyncExecute()

        try await saveRemovedChatTombstoneIfNeeded(for: contact)
    }

    /// A product learns of the removal from its room list, so nothing is sent. Its last chat takes
    /// the product with it, or the worker would boot again only to recreate the room; opening the
    /// product's app installs it again.
    func leaveRoom(_ chat: Chat.LocalModel, of extensionId: ChatExtension.Id) async throws {
        try await deleteChat(chat)

        guard try await chatCount(of: extensionId) == 0 else {
            return
        }

        try await productRepository.saveOperation({ [] }, { [extensionId] }).asyncExecute()
    }

    func deleteChat(_ chat: Chat.LocalModel) async throws {
        try await chatRepository.saveOperation({ [] }, { [chat.chatId.rawRepresentation] }).asyncExecute()
    }

    func chatCount(of extensionId: ChatExtension.Id) async throws -> Int {
        let extensionChatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: .chatsForExtension(extensionId),
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )

        return try await extensionChatRepository.fetchCountOperation().asyncExecute()
    }

    func saveRemovedChatTombstoneIfNeeded(for contact: Chat.Contact) async throws {
        guard messageExchangeModeProvider.mode(for: contact) == .multidevice else {
            return
        }

        let tombstone = Chat.RemovedChat(
            accountId: contact.accountId,
            removedAt: Date()
        )
        try await removedChatRepository.saveOperation({ [tombstone] }, { [] }).asyncExecute()
    }
}
