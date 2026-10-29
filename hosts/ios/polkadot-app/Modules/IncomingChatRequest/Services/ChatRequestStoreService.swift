import Foundation
import Operation_iOS
import MessageExchangeKit
import SubstrateSdk
import KeyDerivation
import StatementStore
import Keystore_iOS

protocol ChatRequestStoreServicing {
    func fetchByRequestId(_ requestId: String) async throws -> Chat.Request?

    func newIncomingRequestFromRemote(
        _ remote: ChatRequest.ValidatedRemoteModel,
        contact: Chat.RemoteContact,
        ownKeyId: Chat.Contact.Own
    ) async throws

    func newOutgoingRequestFromText(
        _ text: String?,
        contact: Chat.RemoteContact,
        ownKeyId: Chat.Contact.Own,
        ownPushToken: Data?
    ) async throws

    func updateRequestFromRemote(_ remote: ChatRequest.ValidatedRemoteModel) async throws

    func replaceRequestFromRemote(_ remote: ChatRequest.ValidatedRemoteModel) async throws

    func acceptIncomingRequest(_ model: ChatRequest.AcceptIncoming) async throws

    func acceptOutgoingRequest(
        _ requestId: String,
        remoteIncoming: ChatRequest.ValidatedRemoteModel?,
        acceptorDevice: Chat.PeerDevice?,
        peerDevice: Chat.PeerDevice?
    ) async throws

    func declineIncomingRequest(_ requestId: String) async throws

    func buildLocalAcceptorDevice() throws -> Chat.PeerDevice
}

final class ChatRequestStoreService {
    let storageFacade: StorageFacadeProtocol
    let pushIdFactory: ChatPushIdMaking
    let deviceEncryptionKeyManager: DeviceEncryptionKeyManaging
    let encryptionManager: MessageExchangeEncryptionManaging
    let walletRepo: WalletManagerRepositoryProtocol

    init(
        storageFacade: StorageFacadeProtocol,
        pushIdFactory: ChatPushIdMaking,
        deviceEncryptionKeyManager: DeviceEncryptionKeyManaging,
        encryptionManager: MessageExchangeEncryptionManaging = ChatEncryptionManager(),
        walletRepo: WalletManagerRepositoryProtocol = .shared
    ) {
        self.storageFacade = storageFacade
        self.pushIdFactory = pushIdFactory
        self.deviceEncryptionKeyManager = deviceEncryptionKeyManager
        self.encryptionManager = encryptionManager
        self.walletRepo = walletRepo
    }
}

extension ChatRequestStoreService: ChatRequestStoreServicing {
    func fetchByRequestId(_ requestId: String) async throws -> Chat.Request? {
        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ChatRequestMapper())
        )
        .fetchOperation(
            by: { requestId },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()
    }

    func newIncomingRequestFromRemote(
        _ remote: ChatRequest.ValidatedRemoteModel,
        contact: Chat.RemoteContact,
        ownKeyId: Chat.Contact.Own
    ) async throws {
        let pushId = pushIdFactory.makePushId(
            peer: contact.toMessageExchangePeer(),
            own: ownKeyId.toMessageExchangeOwn()
        )

        let newIncoming = ChatRequest.NewIncoming(
            remoteRequest: remote,
            remoteContact: contact,
            pushId: pushId?.peerString,
            ownKeyId: ownKeyId
        )

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(NewIncomingChatRequestMapper())
        )
        .saveOperation({ [newIncoming] }, { [] })
        .asyncExecute()
    }

    func newOutgoingRequestFromText(
        _ text: String?,
        contact: Chat.RemoteContact,
        ownKeyId: Chat.Contact.Own,
        ownPushToken: Data?
    ) async throws {
        let pushId = pushIdFactory.makePushId(
            peer: contact.toMessageExchangePeer(),
            own: ownKeyId.toMessageExchangeOwn()
        )

        let pushTokenContent = ownPushToken.map {
            Chat.RemoteTokenContent(token: $0, pushType: .ios)
        }
        let textContent = ChatRemoteMessageContent.RichText(text: text, attachments: nil)

        let identityAccountId = try walletRepo.main().getRawPublicKey()
        let devicePublicKey = try deviceEncryptionKeyManager.getPublicKey()

        let identityProof = try makeIdentityProof(
            identityAccountId: identityAccountId,
            deviceAccountId: identityAccountId,
            peerEncryptionPubKey: contact.chatPublicKey.rawData,
            ownEncryptionKeyId: ownKeyId.encryptionKeyId
        )

        let requestContent = Chat.VersionedRequestContent.v2(Chat.RequestContentV2(
            identityProof: identityProof,
            deviceEncPubKey: devicePublicKey,
            pushToken: pushTokenContent,
            welcomeMessage: textContent
        ))

        let newOutgoing = ChatRequest.NewOutgoing(
            message: Chat.RequestMessage(
                messageId: UUID().uuidString,
                timestamp: Date().toChatTimestamp(),
                content: requestContent
            ),
            remoteContact: contact,
            pushId: pushId?.peerString,
            ownKeyId: ownKeyId
        )

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(NewOutgoingChatRequestMapper())
        )
        .saveOperation({ [newOutgoing] }, { [] })
        .asyncExecute()
    }

    func updateRequestFromRemote(_ remote: ChatRequest.ValidatedRemoteModel) async throws {
        let updateIncoming = ChatRequest.UpdateIncoming(remoteRequest: remote)

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(UpdateIncomingChatRequestMapper())
        )
        .saveOperation({ [updateIncoming] }, { [] })
        .asyncExecute()
    }

    func replaceRequestFromRemote(_ remote: ChatRequest.ValidatedRemoteModel) async throws {
        let replaceIncoming = ChatRequest.ReplaceIncoming(remoteRequest: remote)

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ReplaceIncomingChatRequestMapper())
        )
        .saveOperation({ [replaceIncoming] }, { [] })
        .asyncExecute()
    }

    func acceptIncomingRequest(_ model: ChatRequest.AcceptIncoming) async throws {
        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(AcceptIncomingChatRequestMapper())
        )
        .saveOperation({ [model] }, { [] })
        .asyncExecute()
    }

    func acceptOutgoingRequest(
        _ requestId: String,
        remoteIncoming: ChatRequest.ValidatedRemoteModel?,
        acceptorDevice: Chat.PeerDevice?,
        peerDevice: Chat.PeerDevice?
    ) async throws {
        let acceptOutgoing = ChatRequest.AcceptOutgoing(
            requestId: requestId,
            incomingRequest: remoteIncoming,
            acceptorDevice: acceptorDevice,
            peerDevice: peerDevice
        )

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(AcceptOutgoingChatRequestMapper())
        )
        .saveOperation({ [acceptOutgoing] }, { [] })
        .asyncExecute()
    }

    func declineIncomingRequest(_ requestId: String) async throws {
        let declineIncoming = ChatRequest.DeclineIncoming(requestId: requestId)

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(DeclineIncomingChatRequestMapper())
        )
        .saveOperation({ [declineIncoming] }, { [] })
        .asyncExecute()
    }

    func buildLocalAcceptorDevice() throws -> Chat.PeerDevice {
        try Chat.PeerDevice(
            statementAccountId: walletRepo.main().getRawPublicKey(),
            encryptionPublicKey: deviceEncryptionKeyManager.getPublicKey()
        )
    }
}

private extension ChatRequestStoreService {
    func makeIdentityProof(
        identityAccountId: Data,
        deviceAccountId: Data,
        peerEncryptionPubKey: Data,
        ownEncryptionKeyId: String
    ) throws -> Chat.IdentityProof {
        let encryptor = try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: ownEncryptionKeyId)
            .makeEncryptor(remotePublicKey: peerEncryptionPubKey)

        return try IdentityProofFactory.makeProof(
            identityAccountId: identityAccountId,
            deviceAccountId: deviceAccountId,
            sharedSecret: encryptor.sharedSecret
        )
    }
}
