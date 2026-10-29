import AsyncExtensions
import Foundation
import MessageExchangeKit
import SDKLogger
import StatementDelivery
import StatementStore
import SubstrateSdk

/// An acceptance the peer published for one of our pending outgoing requests.
struct IncomingChatAcceptanceInput: IncomingStatementInput, Equatable {
    let peerAccountId: AccountId
    let requestId: String
    let device: Chat.PeerDevice

    var statementId: String { requestId }
}

/// Receives acceptances outside chat sessions: a requester has no session until it knows the acceptor's device.
final class IncomingChatAcceptanceService: @unchecked Sendable {
    private let incomingService: IncomingStatementService<MessageExchange.SessionRequest, IncomingChatAcceptanceInput>

    init(
        encryptionManager: MessageExchangeEncryptionManaging,
        signerManager: StatementStoreSignerManaging,
        connection: StatementStoreConnecting,
        logger: SDKLoggerProtocol
    ) {
        incomingService = IncomingStatementService(
            decoder: ChatAcceptanceStatementDecoder(encryptionManager: encryptionManager, signerManager: signerManager),
            connection: connection,
            logger: logger,
            workQueue: DispatchQueue(label: "io.chat.acceptance.incoming.queue")
        )
    }
}

extension IncomingChatAcceptanceService: IncomingStatementServicing {
    /// Acceptances the peers of pending outgoing requests publish on the identity route.
    func subscribe(to targets: [MessageExchange.SessionRequest]) -> AnyAsyncSequence<IncomingChatAcceptanceInput> {
        incomingService.subscribe(to: targets)
    }
}

/// Decodes the peer's identity-route requests; routed by topic only, since every period's copy has its own signer.
private struct ChatAcceptanceStatementDecoder: IncomingStatementDecoding, @unchecked Sendable {
    let encryptionManager: MessageExchangeEncryptionManaging
    let signerManager: StatementStoreSignerManaging
    var sessionIdFactory: PeerSessionIdFactoryProtocol = PeerSessionIdFactory()

    func topicFilter(for target: MessageExchange.SessionRequest) throws -> TopicFilter {
        let sessionId = try makeSessionId(for: target, encryptor: makeEncryptor(for: target))
        return try .matchAll([sessionId.peer.fixedStatementFieldData()])
    }

    func decode(
        _ statement: Statement,
        for target: MessageExchange.SessionRequest
    ) throws -> [IncomingChatAcceptanceInput] {
        guard let payload = statement.getScaleEncodedPayload() else { return [] }

        let result: StatementDataDecodingResult<Chat.OpaqueMessage> = try StatementDataCoder(
            encryptor: makeEncryptor(for: target),
            logger: nil
        )
        .decodeFromScaleEncodedPayload(payload, senderAccountId: nil, route: .identity)

        guard case let .statementData(.request(peerRequest)) = result else { return [] }

        return peerRequest.messages.compactMap { message in
            guard case let .multiChatAccepted(accepted) = message.remoteMessage.versioned.ensureV1()?.content else {
                return nil
            }

            return IncomingChatAcceptanceInput(
                peerAccountId: target.peer.accountId,
                requestId: accepted.requestId,
                device: accepted.device
            )
        }
    }

    private func makeEncryptor(for session: MessageExchange.SessionRequest) throws -> MessageExchangeEncrypting {
        try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: session.own.encryptionKeyId)
            .makeEncryptor(remotePublicKey: session.peer.publicKey)
    }

    private func makeSessionId(
        for session: MessageExchange.SessionRequest,
        encryptor: MessageExchangeEncrypting
    ) throws -> MessageExchange.SessionId {
        try sessionIdFactory.createSessionId(
            for: .init(
                ownAccountId: signerManager.makeSigner(for: session.own.signKeyId).accountId,
                ownPin: session.own.pin,
                peerAccountId: session.peer.accountId,
                peerPin: session.peer.pin,
                sharedSecret: encryptor.sharedSecret
            )
        )
    }
}
