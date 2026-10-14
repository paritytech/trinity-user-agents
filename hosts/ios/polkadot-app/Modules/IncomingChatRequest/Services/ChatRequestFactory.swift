import Foundation
import SubstrateSdk
import NovaCrypto
import MessageExchangeKit
import StatementStore
import CryptoKit
import Keystore_iOS

protocol ChatRequestFactoryProtocol {
    func createRemoteRequest(
        from message: Chat.RequestMessage,
        peerEncryptionPubKey: Data,
        peerAccountId: AccountId,
        ownKeyId: MessageExchange.Own
    ) throws -> ChatRequest.EncryptedRemoteModel

    func decodeAndValidate(
        remotePayload: Data,
        ownKeyId: Chat.Contact.Own
    ) async throws -> ChatRequest.ValidatedRemoteModel
}

enum ChatRequestFactoryError: Error {
    case unsupportedProof
    case invalidSignature
    case peerNotFound
}

final class ChatRequestFactory {
    let encryptionManager: MessageExchangeEncryptionManaging
    let signManager: StatementStoreSignerManaging
    let remoteContactResolver: RemoteContactResolving

    init(
        encryptionManager: MessageExchangeEncryptionManaging,
        signManager: StatementStoreSignerManaging,
        remoteContactResolver: RemoteContactResolving
    ) {
        self.encryptionManager = encryptionManager
        self.signManager = signManager
        self.remoteContactResolver = remoteContactResolver
    }
}

extension ChatRequestFactory: ChatRequestFactoryProtocol {
    func createRemoteRequest(
        from message: Chat.RequestMessage,
        peerEncryptionPubKey: Data,
        peerAccountId: AccountId,
        ownKeyId: MessageExchange.Own
    ) throws -> ChatRequest.EncryptedRemoteModel {
        let messageData = try ChatRequest.ProofPayload(
            message: message,
            requestAcceptorId: peerAccountId
        )
        .scaleEncoded()

        let proof = try signManager.makeSigner(for: ownKeyId.signKeyId).sign(messageData)

        let remoteModel = ChatRequest.RemoteModel(
            message: message,
            proof: proof
        )

        let tempPrivateKey = Curve25519.KeyAgreement.PrivateKey()
        let tempEncryptorFactory = X25519ChaChaPolyEncryptorFactory(privateKey: tempPrivateKey)
        let tempEncryptor = try tempEncryptorFactory.makeEncryptor(remotePublicKey: peerEncryptionPubKey)

        let dataToEncrypt = try remoteModel.scaleEncoded()
        let encryptedData = try tempEncryptor.encrypt(dataToEncrypt)

        return ChatRequest.EncryptedRemoteModel(
            encryptionPubKey: tempEncryptorFactory.localPublicKey,
            encryptedData: encryptedData
        )
    }

    func decodeAndValidate(
        remotePayload: Data,
        ownKeyId: Chat.Contact.Own
    ) async throws -> ChatRequest.ValidatedRemoteModel {
        let encryptedModel = try ChatRequest.EncryptedRemoteModel(
            scaleDecoder: ScaleDecoder(data: remotePayload)
        )

        let decryptor = try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: ownKeyId.encryptionKeyId)
            .makeEncryptor(remotePublicKey: encryptedModel.encryptionPubKey)

        let decryptedData = try decryptor.decrypt(encryptedModel.encryptedData)

        let decryptedModel = try ChatRequest.RemoteModel(scaleDecoder: ScaleDecoder(data: decryptedData))

        let myAccountId = try signManager.makeSigner(for: ownKeyId.signKeyId).accountId

        guard case let .sr25519(signature, signer) = decryptedModel.proof else {
            throw ChatRequestFactoryError.unsupportedProof
        }

        let verifier = SNSignatureVerifier()
        let signatureModel = try SNSignature(rawData: signature)

        let signedData = try ChatRequest.ProofPayload(
            message: decryptedModel.message,
            requestAcceptorId: myAccountId
        )
        .scaleEncoded()

        let signerPublicKey = try SNPublicKey(rawData: signer)

        guard verifier.verify(signatureModel, forOriginalData: signedData, using: signerPublicKey) else {
            throw ChatRequestFactoryError.invalidSignature
        }

        let peerAccountId = try await resolvePeerAccountId(
            message: decryptedModel.message,
            signer: signer,
            ownEncryptionKeyId: ownKeyId.encryptionKeyId
        )

        return ChatRequest.ValidatedRemoteModel(
            message: decryptedModel.message,
            peerAccountId: peerAccountId,
            // TODO: Add statement account id to request.
            // Taking `signer` is fine for now and needed for device sync,
            // but this is an assumption.
            peerStatementAccountId: signer
        )
    }
}

private extension ChatRequestFactory {
    func resolvePeerAccountId(
        message: Chat.RequestMessage,
        signer: Data,
        ownEncryptionKeyId: String
    ) async throws -> AccountId {
        guard let identityAccountId = message.content.extractIdentityAccountId() else {
            return signer
        }

        let peerEncryptionPubKey = try await fetchPeerEncryptionPubKey(
            for: identityAccountId
        )

        try verifyIdentityProof(
            message,
            deviceAccountId: signer,
            peerEncryptionPubKey: peerEncryptionPubKey,
            ownEncryptionKeyId: ownEncryptionKeyId
        )

        return identityAccountId
    }

    func fetchPeerEncryptionPubKey(for accountId: AccountId) async throws -> Data {
        guard let remoteContact = try await remoteContactResolver.fetch(by: accountId) else {
            throw ChatRequestFactoryError.peerNotFound
        }
        return remoteContact.chatPublicKey.rawData
    }

    func verifyIdentityProof(
        _ message: Chat.RequestMessage,
        deviceAccountId: Data,
        peerEncryptionPubKey: Data,
        ownEncryptionKeyId: String
    ) throws {
        guard case let .v2(content) = message.content else { return }

        let encryptor = try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: ownEncryptionKeyId)
            .makeEncryptor(remotePublicKey: peerEncryptionPubKey)

        try IdentityProofFactory.verifyProof(
            content.identityProof,
            deviceAccountId: deviceAccountId,
            sharedSecret: encryptor.sharedSecret
        )
    }
}
