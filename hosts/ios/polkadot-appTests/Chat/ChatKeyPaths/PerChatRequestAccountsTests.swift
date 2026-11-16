import CryptoKit
import Foundation
import MessageExchangeKit
import StatementStore
import SubstrateSdk
import Testing
@testable import polkadot_app

struct PerChatRequestAccountsTests {
    private let entropyManager = FixedRootEntropyManager()
    private let peerAccountId = Data(repeating: 0xAB, count: 32)
    private let ownSignKeyId = try! WalletDerivationPath.main(for: "dot")

    @Test func innerRequestProofIsSignedByIdentityAccount() throws {
        let acceptorKey = Curve25519.KeyAgreement.PrivateKey()

        let encrypted = try makeRequestFactory().createRemoteRequest(
            from: Chat.RequestMessage(
                messageId: UUID().uuidString,
                timestamp: Date().toChatTimestamp(),
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            peerEncryptionPubKey: acceptorKey.publicKey.rawRepresentation,
            peerAccountId: peerAccountId,
            ownSignKeyId: ownSignKeyId
        )

        let decrypted = try X25519ChaChaPolyEncryptorFactory(privateKey: acceptorKey)
            .makeEncryptor(remotePublicKey: encrypted.encryptionPubKey)
            .decrypt(encrypted.encryptedData)
        let remote = try ChatRequest.RemoteModel(scaleDecoder: ScaleDecoder(data: decrypted))

        guard case let .sr25519(_, signer) = remote.proof else {
            Issue.record("Unexpected proof type")
            return
        }

        #expect(try signer == identityAccountId())
    }

    @Test func acceptorRejectsRequestWithoutDevice() async throws {
        let acceptorKey = Curve25519.KeyAgreement.PrivateKey()
        let ownKeyId = Chat.Contact.Own(
            signKeyId: ownSignKeyId,
            encryptionKeyId: ChatEncryptionDomain.mainChat.rawValue
        )
        let acceptorAccountId = try ChatSignerManager(entropyManager: entropyManager)
            .makeSigner(for: ownKeyId.signKeyId)
            .accountId

        let encrypted = try makeRequestFactory().createRemoteRequest(
            from: Chat.RequestMessage(
                messageId: UUID().uuidString,
                timestamp: Date().toChatTimestamp(),
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            peerEncryptionPubKey: acceptorKey.publicKey.rawRepresentation,
            peerAccountId: acceptorAccountId,
            ownSignKeyId: ownSignKeyId
        )

        let acceptorFactory = ChatRequestFactory(
            encryptionManager: ClosureEncryptionManager { _ in
                X25519ChaChaPolyEncryptorFactory(privateKey: acceptorKey)
            },
            signManager: ChatSignerManager(entropyManager: entropyManager),
            remoteContactResolver: MockRemoteContactResolver(result: .success(nil))
        )

        await #expect(throws: ChatRequestFactoryError.unsupportedRequestVersion) {
            try await acceptorFactory.decodeAndValidate(remotePayload: encrypted.scaleEncoded(), ownKeyId: ownKeyId)
        }
    }

    @Test func acceptorValidatesCancelWithIdentityProof() async throws {
        let senderKey = Curve25519.KeyAgreement.PrivateKey()
        let acceptorKey = Curve25519.KeyAgreement.PrivateKey()
        let messageId = UUID().uuidString
        let identityProof = try makeCancelProof(messageId: messageId, senderKey: senderKey, acceptorKey: acceptorKey)

        let validated = try await decodeCancel(
            identityProof: identityProof,
            messageId: messageId,
            senderKey: senderKey,
            acceptorKey: acceptorKey
        )

        let senderAccountId = try identityAccountId()
        #expect(validated.peerAccountId == senderAccountId)
        #expect(validated.message.content.isCancel)
    }

    @Test func acceptorRejectsCancelWithTamperedIdentityProof() async throws {
        let senderKey = Curve25519.KeyAgreement.PrivateKey()
        let acceptorKey = Curve25519.KeyAgreement.PrivateKey()
        let messageId = UUID().uuidString
        let validProof = try makeCancelProof(messageId: messageId, senderKey: senderKey, acceptorKey: acceptorKey)
        var tamperedBytes = validProof.proof
        tamperedBytes[0] ^= 0x01
        let tamperedProof = Chat.IdentityProof(identityAccountId: validProof.identityAccountId, proof: tamperedBytes)

        await #expect(throws: IdentityProofError.invalidProof) {
            try await decodeCancel(
                identityProof: tamperedProof,
                messageId: messageId,
                senderKey: senderKey,
                acceptorKey: acceptorKey
            )
        }
    }

    @Test func acceptorRejectsCancelProofForAnotherRequest() async throws {
        let senderKey = Curve25519.KeyAgreement.PrivateKey()
        let acceptorKey = Curve25519.KeyAgreement.PrivateKey()
        let identityProof = try makeCancelProof(
            messageId: UUID().uuidString,
            senderKey: senderKey,
            acceptorKey: acceptorKey
        )

        await #expect(throws: IdentityProofError.invalidProof) {
            try await decodeCancel(
                identityProof: identityProof,
                messageId: UUID().uuidString,
                senderKey: senderKey,
                acceptorKey: acceptorKey
            )
        }
    }

    @Test func cancelProofMatchesKnownVector() throws {
        let proof = try IdentityProofFactory.makeCancelProof(
            identityAccountId: Data(repeating: 0x07, count: 32),
            requestId: "00000000-0000-0000-0000-000000000001",
            sharedSecret: Data(repeating: 0x01, count: 32)
        )

        #expect(proof.proof.toHex() == "c58f1326d87a7778cbdfbf08592ee72ff2eebb3837b64c5cd0978eba5928bcad")
    }

    @Test func acceptorAdvertisesIdentityAccountAsDevice() throws {
        let deviceKeyManager = StubDeviceEncryptionKeyManager()
        let service = ChatRequestStoreService(
            storageFacade: UserDataStorageTestFacade(),
            pushIdFactory: ChatPushIdFactory(
                encryptionManager: ChatEncryptionManager(entropyManager: entropyManager),
                signManager: ChatSignerManager(entropyManager: entropyManager),
                sessionIdFactory: PeerSessionIdFactory(),
                logger: MockLogger()
            ),
            deviceEncryptionKeyManager: deviceKeyManager,
            encryptionManager: ChatEncryptionManager(entropyManager: entropyManager),
            walletRepo: MockWalletManagerRepository(entropyManager: entropyManager)
        )
        let device = try service.buildLocalAcceptorDevice()

        #expect(try device.statementAccountId == identityAccountId())
        #expect(try device.encryptionPublicKey == (deviceKeyManager.getPublicKey()))
    }
}

private extension PerChatRequestAccountsTests {
    func makeRequestFactory() -> ChatRequestFactory {
        ChatRequestFactory(
            encryptionManager: ChatEncryptionManager(entropyManager: entropyManager),
            signManager: ChatSignerManager(entropyManager: entropyManager),
            remoteContactResolver: MockRemoteContactResolver(result: .success(nil))
        )
    }

    func identityAccountId() throws -> AccountId {
        try ChatSignerManager(entropyManager: entropyManager)
            .makeSigner(for: ownSignKeyId)
            .accountId
    }

    func makeSharedSecret(
        senderKey: Curve25519.KeyAgreement.PrivateKey,
        acceptorKey: Curve25519.KeyAgreement.PrivateKey
    ) throws -> Data {
        try X25519ChaChaPolyEncryptorFactory(privateKey: senderKey)
            .makeEncryptor(remotePublicKey: acceptorKey.publicKey.rawRepresentation)
            .sharedSecret
    }

    func makeCancelProof(
        messageId: String,
        senderKey: Curve25519.KeyAgreement.PrivateKey,
        acceptorKey: Curve25519.KeyAgreement.PrivateKey
    ) throws -> Chat.IdentityProof {
        let sharedSecret = try makeSharedSecret(senderKey: senderKey, acceptorKey: acceptorKey)

        return try IdentityProofFactory.makeCancelProof(
            identityAccountId: identityAccountId(),
            requestId: messageId,
            sharedSecret: sharedSecret
        )
    }

    func decodeCancel(
        identityProof: Chat.IdentityProof,
        messageId: String,
        senderKey: Curve25519.KeyAgreement.PrivateKey,
        acceptorKey: Curve25519.KeyAgreement.PrivateKey
    ) async throws -> ChatRequest.ValidatedRemoteModel {
        let acceptorSignKeyId = try WalletDerivationPath.main(for: "paseo")
        let acceptorAccountId = try ChatSignerManager(entropyManager: entropyManager)
            .makeSigner(for: acceptorSignKeyId)
            .accountId

        let encrypted = try makeRequestFactory().createRemoteRequest(
            from: Chat.RequestMessage(
                messageId: messageId,
                timestamp: Date().toChatTimestamp(),
                content: .cancel(Chat.RequestContentCancel(identityProof: identityProof))
            ),
            peerEncryptionPubKey: acceptorKey.publicKey.rawRepresentation,
            peerAccountId: acceptorAccountId,
            ownSignKeyId: ownSignKeyId
        )

        let senderContact = try Chat.RemoteContact(
            accountId: identityProof.identityAccountId,
            username: "sender",
            chatPublicKey: Chat.PublicKey(rawData: senderKey.publicKey.rawRepresentation),
            imageData: nil
        )
        let acceptorFactory = ChatRequestFactory(
            encryptionManager: ClosureEncryptionManager { _ in
                X25519ChaChaPolyEncryptorFactory(privateKey: acceptorKey)
            },
            signManager: ChatSignerManager(entropyManager: entropyManager),
            remoteContactResolver: MockRemoteContactResolver(result: .success(senderContact))
        )
        let acceptorOwnKeyId = Chat.Contact.Own(
            signKeyId: acceptorSignKeyId,
            encryptionKeyId: ChatEncryptionDomain.mainChat.rawValue
        )

        return try await acceptorFactory.decodeAndValidate(
            remotePayload: encrypted.scaleEncoded(),
            ownKeyId: acceptorOwnKeyId
        )
    }
}
