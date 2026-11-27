import Foundation
import Operation_iOS
import Foundation_iOS
import MessageExchangeKit

enum ChatRequestEngineContextError: Error {
    case missingChat
    case noIncomingRequest
    case noOutgoingRequest
    case noPendingOutgoingRequest
    case unsupportedContent
}

actor ChatRequestEngineContext {
    private var pendingRequest: ChatOpenModel.NewRequest?
    private var incomingRequestId: String?

    let tokenProvider: APNSTokenProviding
    let chatRequestStoreService: ChatRequestStoreServicing
    let cancelService: ChatRequestCancelServicing
    let chatRepository: AnyDataProviderRepository<Chat.LocalModel>

    init(
        pendingRequest: ChatOpenModel.NewRequest?,
        chatRequestStoreService: ChatRequestStoreServicing,
        cancelService: ChatRequestCancelServicing,
        tokenProvider: APNSTokenProviding,
        storageFacade: StorageFacadeProtocol
    ) {
        self.pendingRequest = pendingRequest
        self.chatRequestStoreService = chatRequestStoreService
        self.cancelService = cancelService
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
                icon: .person(
                    username: pendingRequest.remoteContact.username,
                    imageData: pendingRequest.remoteContact.imageData
                ),
                input: .outgoingRequest,
                moreActions: []
            ),
            state: .pending,
            peerSeenOrder: 0
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
        let request = try await fetchIncomingChatContact(for: chatId)
        let acceptorDevice = try chatRequestStoreService.buildLocalAcceptorDevice()
        try await chatRequestStoreService.acceptIncomingRequest(
            .existing(requestId: request.requestId, acceptorDevice: acceptorDevice)
        )
    }

    func declineIncomingRequest(for chatId: Chat.Id) async throws {
        let request = try await fetchIncomingChatContact(for: chatId)
        try await chatRequestStoreService.declineIncomingRequest(request.requestId)
    }

    func cancelOutgoingRequest(for chatId: Chat.Id) async throws {
        let request = try await fetchOutgoingChatRequest(for: chatId)
        try await cancelService.cancel(requestId: request.requestId)
    }

    func rememberIncomingRequest(of chat: Chat.LocalModel) {
        incomingRequestId = chat.peer.contact?.chatRequest.flatMap { $0.isIncoming ? $0.requestId : nil }
    }

    func removalReason() async -> ChatRemovalReason {
        guard
            let incomingRequestId,
            let request = try? await chatRequestStoreService.fetchByRequestId(incomingRequestId),
            request.status == .incoming(.cancelled)
        else {
            return .other
        }

        return .requestCancelled
    }
}

private extension ChatRequestEngineContext {
    func fetchOutgoingChatRequest(for chatId: Chat.Id) async throws -> Chat.Request {
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
            request.isOutgoing
        else {
            throw ChatRequestEngineContextError.noOutgoingRequest
        }

        return request
    }

    func fetchIncomingChatContact(for chatId: Chat.Id) async throws -> Chat.Request {
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

        return request
    }
}
