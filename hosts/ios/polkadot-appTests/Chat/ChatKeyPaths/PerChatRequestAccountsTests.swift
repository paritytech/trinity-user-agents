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
    private let ownSignKeyId = WalletDerivationPath.main(for: "dot")

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
}
