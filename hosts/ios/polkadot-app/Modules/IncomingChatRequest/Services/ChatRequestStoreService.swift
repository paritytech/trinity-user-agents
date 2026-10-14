import Foundation
import Operation_iOS
import MessageExchangeKit
import SubstrateSdk
import KeyDerivation
import StatementStore
import Keystore_iOS

protocol ChatRequestStoreServicing {
    func fetchByRequestId(_ requestId: String) async throws -> Chat.Request?

    func fetchPeerDeviceByRequestId(_ requestId: String) async throws -> Chat.PeerDevice?

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
        messageExchangeMode: MessageExchangeMode,
        remoteIncoming: ChatRequest.ValidatedRemoteModel?,
        acceptorDevice: Chat.PeerDevice?
    ) async throws

    func declineIncomingRequest(_ requestId: String) async throws

    func buildLocalAcceptorDevice(for ownKeyId: Chat.Contact.Own) throws -> Chat.PeerDevice?
}

final class ChatRequestStoreService {
    let storageFacade: StorageFacadeProtocol
    let pushIdFactory: ChatPushIdMaking
    let deviceEncryptionKeyManager: DeviceEncryptionKeyManaging
    let messageExchangeModeProvider: MessageExchangeModeProviding
    let encryptionManager: MessageExchangeEncryptionManaging
    let signManager: StatementStoreSignerManaging
    let walletRepo: WalletManagerRepositoryProtocol

    init(
        messageExchangeModeProvider: MessageExchangeModeProviding,
        storageFacade: StorageFacadeProtocol,
        pushIdFactory: ChatPushIdMaking,
        deviceEncryptionKeyManager: DeviceEncryptionKeyManaging,
        encryptionManager: MessageExchangeEncryptionManaging = ChatEncryptionManager(),
        signManager: StatementStoreSignerManaging = ChatSignerManager(),
        walletRepo: WalletManagerRepositoryProtocol = .shared
    ) {
        self.storageFacade = storageFacade
        self.pushIdFactory = pushIdFactory
        self.deviceEncryptionKeyManager = deviceEncryptionKeyManager
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.encryptionManager = encryptionManager
        self.signManager = signManager
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

    func fetchPeerDeviceByRequestId(_ requestId: String) async throws -> Chat.PeerDevice? {
        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ChatRequestPeerDeviceMapper())
        )
        .fetchOperation(
            by: { requestId },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()?
        .peerDevice
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

        let requestContent: Chat.VersionedRequestContent

        switch messageExchangeModeProvider.mode(for: ownKeyId) {
        case .identity:
            requestContent = .v1(Chat.RequestContentV1(
                pushToken: pushTokenContent,
                welcomeMessage: textContent
            ))
        case .multidevice:
            let identityAccountId = try walletRepo.main().getRawPublicKey()
            let devicePublicKey = try deviceEncryptionKeyManager.getPublicKey()

            let signer = try signManager.makeSigner(for: ownKeyId.signKeyId)
            let identityProof = try makeIdentityProof(
                identityAccountId: identityAccountId,
                deviceAccountId: signer.accountId,
                peerEncryptionPubKey: contact.chatPublicKey.rawData,
                ownEncryptionKeyId: ownKeyId.encryptionKeyId
            )

            requestContent = .v2(Chat.RequestContentV2(
                identityProof: identityProof,
                deviceEncPubKey: devicePublicKey,
                pushToken: pushTokenContent,
                welcomeMessage: textContent
            ))
        }

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
        messageExchangeMode: MessageExchangeMode,
        remoteIncoming: ChatRequest.ValidatedRemoteModel?,
        acceptorDevice: Chat.PeerDevice?
    ) async throws {
        let acceptOutgoing = ChatRequest.AcceptOutgoing(
            requestId: requestId,
            messageExchangeMode: messageExchangeMode,
            incomingRequest: remoteIncoming,
            acceptorDevice: acceptorDevice
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

    func buildLocalAcceptorDevice(for ownKeyId: Chat.Contact.Own) throws -> Chat.PeerDevice? {
        switch messageExchangeModeProvider.mode(for: ownKeyId) {
        case .identity:
            return nil
        case .multidevice:
            let identityAccountId = try walletRepo.main().getRawPublicKey()
            let devicePublicKey = try deviceEncryptionKeyManager.getPublicKey()

            return Chat.PeerDevice(
                statementAccountId: identityAccountId,
                encryptionPublicKey: devicePublicKey
            )
        }
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
