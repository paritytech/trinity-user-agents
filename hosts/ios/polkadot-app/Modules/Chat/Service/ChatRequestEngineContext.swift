import Foundation
import Operation_iOS
import Foundation_iOS
import MessageExchangeKit

enum ChatRequestEngineContextError: Error {
    case missingChat
    case noIncomingRequest
    case noPendingOutgoingRequest
    case unsupportedContent
}

actor ChatRequestEngineContext {
    private var pendingRequest: ChatOpenModel.NewRequest?

    let tokenProvider: APNSTokenProviding
    let chatRequestStoreService: ChatRequestStoreServicing
    let messageExchangeModeProvider: any MessageExchangeModeProviding
    let chatRepository: AnyDataProviderRepository<Chat.LocalModel>

    init(
        pendingRequest: ChatOpenModel.NewRequest?,
        chatRequestStoreService: ChatRequestStoreServicing,
        messageExchangeModeProvider: any MessageExchangeModeProviding,
        tokenProvider: APNSTokenProviding,
        storageFacade: StorageFacadeProtocol
    ) {
        self.pendingRequest = pendingRequest
        self.chatRequestStoreService = chatRequestStoreService
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.tokenProvider = tokenProvider

        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )
    }

    func hasPendingRequest() -> Bool {
        pendingRequest != nil
    }

    func resetPendingOutgoingRequest() {
        pendingRequest = nil
    }

    func chatMetadataForPendingRequest() -> ChatMetadata? {
        guard let pendingRequest else {
            return nil
        }

        return ChatMetadata(
            chatId: .person(pendingRequest.remoteContact.accountId),
            peerMetadata: Chat.PeerMetadata(
                name: pendingRequest.remoteContact.username,
                icon: .image(pendingRequest.remoteContact.imageData),
                input: .outgoingRequest,
                moreActions: []
            ),
            state: .pending
        )
    }

    func submitPendingOutgoingRequest(with content: Chat.LocalMessage.Content) async throws {
        guard let pendingRequest else {
            throw ChatRequestEngineContextError.noPendingOutgoingRequest
        }

        guard case let .text(text) = content else {
            throw ChatRequestEngineContextError.unsupportedContent
        }

        try await chatRequestStoreService.newOutgoingRequestFromText(
            text.isEmpty ? nil : text,
            contact: pendingRequest.remoteContact,
            ownKeyId: pendingRequest.ownKeyId,
            ownPushToken: tokenProvider.currentToken
        )
    }

    func acceptIncomingRequest(for chatId: Chat.Id) async throws {
        let (contact, request) = try await fetchIncomingChatContact(for: chatId)
        let messageExchangeMode = messageExchangeModeProvider.mode(for: contact.ownKeyId)
        let acceptorDevice = try chatRequestStoreService.buildLocalAcceptorDevice(for: contact.ownKeyId)
        try await chatRequestStoreService.acceptIncomingRequest(
            .existing(
                requestId: request.requestId,
                messageExchangeMode: messageExchangeMode,
                acceptorDevice: acceptorDevice
            )
        )
    }

    func declineIncomingRequest(for chatId: Chat.Id) async throws {
        let (_, request) = try await fetchIncomingChatContact(for: chatId)
        try await chatRequestStoreService.declineIncomingRequest(request.requestId)
    }
}

private extension ChatRequestEngineContext {
    func fetchIncomingChatContact(for chatId: Chat.Id) async throws -> (Chat.Contact, Chat.Request) {
        let chat = try await chatRepository
            .fetchOperation(
                by: { chatId.rawRepresentation },
                options: RepositoryFetchOptions()
            )
            .asyncExecute()
            .mapOrThrow(ChatRequestEngineContextError.missingChat)

        guard
            let contact = chat.peer.contact,
            let request = contact.chatRequest,
            request.isIncoming
        else {
            throw ChatRequestEngineContextError.noIncomingRequest
        }

        return (contact, request)
    }
}
