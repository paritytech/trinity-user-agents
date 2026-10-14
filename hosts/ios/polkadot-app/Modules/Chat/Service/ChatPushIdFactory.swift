import Foundation
import MessageExchangeKit
import StatementStore
import CryptoKit

protocol ChatPushIdMaking {
    func makePushId(peer: MessageExchange.Peer, own: MessageExchange.Own) -> Chat.PushId?
}

final class ChatPushIdFactory {
    private let encryptionManager: MessageExchangeEncryptionManaging
    private let signManager: StatementStoreSignerManaging
    private let sessionIdFactory: PeerSessionIdFactoryProtocol
    private let logger: LoggerProtocol

    init(
        encryptionManager: MessageExchangeEncryptionManaging,
        signManager: StatementStoreSignerManaging,
        sessionIdFactory: PeerSessionIdFactoryProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.encryptionManager = encryptionManager
        self.signManager = signManager
        self.sessionIdFactory = sessionIdFactory
        self.logger = logger
    }
}

extension ChatPushIdFactory: ChatPushIdMaking {
    func makePushId(peer: MessageExchange.Peer, own: MessageExchange.Own) -> Chat.PushId? {
        do {
            let secret = try encryptionManager
                .makeEncryptorFactory(ownEncryptionKeyId: own.encryptionKeyId)
                .makeEncryptor(remotePublicKey: peer.publicKey)
                .sharedSecret

            let signer = try signManager.makeSigner(for: own.signKeyId)

            let sessionId = try sessionIdFactory.createSessionId(
                for: .init(
                    ownAccountId: signer.accountId,
                    ownPin: own.pin,
                    peerAccountId: peer.accountId,
                    peerPin: peer.pin,
                    sharedSecret: secret
                )
            )

            let own = try makeRawPushId(parameter: sessionId.ownParameter, secret: secret)
            let peer = try makeRawPushId(parameter: sessionId.peerParameter, secret: secret)
            return Chat.PushId(own: own, peer: peer)
        } catch {
            logger.error("Error: \(error)")
            return nil
        }
    }
}

private extension ChatPushIdFactory {
    func makeRawPushId(parameter: Data, secret: Data) throws -> Data {
        let dataToHash = Data("notification".utf8) + parameter
        return try dataToHash.blake2b32WithKey(secret)
    }
}
