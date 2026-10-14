import Foundation
import NovaCrypto
import MessageExchangeKit
import StatementStore
import CryptoKit

protocol ChatRequestChannelFactoryProtocol {
    func outgoingChannel(
        with peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> StatementFixedFieldConvertible

    func incomingChannel(
        with peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> StatementFixedFieldConvertible
}

final class ChatRequestChannelFactory {
    let sessionIdFactory: PeerSessionIdFactoryProtocol
    let encryptionManager: MessageExchangeEncryptionManaging
    let signingManager: StatementStoreSignerManaging

    init(
        encryptionManager: MessageExchangeEncryptionManaging,
        signingManager: StatementStoreSignerManaging,
        sessionIdFactory: PeerSessionIdFactoryProtocol
    ) {
        self.encryptionManager = encryptionManager
        self.signingManager = signingManager
        self.sessionIdFactory = sessionIdFactory
    }
}

private extension ChatRequestChannelFactory {
    func makeChannel(from sessionIdParam: Data, sharedSecret: Data) throws -> Data {
        let dataToHash = ChatRequest.statementStoreContext + sessionIdParam
        return try dataToHash.blake2b32WithKey(sharedSecret)
    }

    func makeChannel(
        with peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own,
        isOutgoing: Bool
    ) throws -> StatementFixedFieldConvertible {
        let encryptor = try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: ownKeyId.encryptionKeyId)
            .makeEncryptor(remotePublicKey: peer.publicKey)

        let signer = try signingManager.makeSigner(for: ownKeyId.signKeyId)

        let sessionId = try sessionIdFactory.createSessionId(
            for: .init(
                ownAccountId: signer.accountId,
                ownPin: ownKeyId.pin,
                peerAccountId: peer.accountId,
                peerPin: peer.pin,
                sharedSecret: encryptor.sharedSecret
            )
        )

        let sessionIdParam = isOutgoing ? sessionId.ownParameter : sessionId.peerParameter

        return try makeChannel(from: sessionIdParam, sharedSecret: encryptor.sharedSecret)
    }
}

extension ChatRequestChannelFactory: ChatRequestChannelFactoryProtocol {
    func outgoingChannel(
        with peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> StatementFixedFieldConvertible {
        try makeChannel(with: peer, ownKeyId: ownKeyId, isOutgoing: true)
    }

    func incomingChannel(
        with peer: MessageExchange.Peer,
        ownKeyId: MessageExchange.Own
    ) throws -> StatementFixedFieldConvertible {
        try makeChannel(with: peer, ownKeyId: ownKeyId, isOutgoing: false)
    }
}
