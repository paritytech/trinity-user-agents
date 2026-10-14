import Foundation
import Foundation_iOS
import Operation_iOS
import SubstrateSdk
import MessageExchangeKit
import StatementStore
import AsyncExtensions
import SDKLogger
import SubstrateSdkExt

protocol IncomingChatRequestServicing {
    func subscribe(
        peers: Set<MessageExchange.Peer>,
        ownKeyId: Chat.Contact.Own
    ) -> AnyAsyncSequence<ChatRequest.ValidatedRemoteModel>
}

enum IncomingChatRequestServiceError: Error {
    case noPayload
}

final class IncomingChatRequestService {
    private let statementStoreConnection: StatementStoreConnecting
    private let channelFactory: ChatRequestChannelFactoryProtocol
    private let chatRequestFactory: ChatRequestFactoryProtocol
    private let logger: SDKLoggerProtocol

    private let pollDispatchQueue = DispatchQueue(label: "io.incoming.chat.request.poll.queue")
    private let pollOperationQueue = OperationQueue()

    init(
        statementStoreConnection: StatementStoreConnecting,
        channelFactory: ChatRequestChannelFactoryProtocol,
        chatRequestFactory: ChatRequestFactoryProtocol,
        logger: SDKLoggerProtocol
    ) {
        self.statementStoreConnection = statementStoreConnection
        self.channelFactory = channelFactory
        self.chatRequestFactory = chatRequestFactory
        self.logger = logger
    }
}

extension IncomingChatRequestService: IncomingChatRequestServicing {
    func subscribe(
        peers: Set<MessageExchange.Peer>,
        ownKeyId: Chat.Contact.Own
    ) -> AnyAsyncSequence<ChatRequest.ValidatedRemoteModel> {
        let stream = AsyncStream<ChatRequest.ValidatedRemoteModel> { continuation in
            let pollers = peers.compactMap { peer in
                do {
                    return try createPoller(for: peer, ownKeyId: ownKeyId)
                } catch {
                    logger.error("Can't create poller for: \(error)")
                    return nil
                }
            }

            logger.debug("Starting pollers: \(pollers.count)")

            pollers.forEach { poller in
                poller.start { [weak self] statement in
                    guard let self else { return false }

                    Task {
                        do {
                            let model = try await self.handleStatement(statement, ownKeyId: ownKeyId)

                            let peerAddress = try model.peerAccountId.toAddress(using: .genericFormat)
                            self.logger.debug("Received chat request from \(peerAddress)")

                            continuation.yield(model)
                        } catch {
                            self.logger.error("Failed to handle statement: \(error)")
                        }
                    }

                    return true
                }
            }

            continuation.onTermination = { _ in
                pollers.forEach { $0.stop() }
            }
        }

        return stream.eraseToAnyAsyncSequence()
    }
}

private extension IncomingChatRequestService {
    func createPoller(
        for peer: MessageExchange.Peer,
        ownKeyId: Chat.Contact.Own
    ) throws -> StatementSubscription {
        let topic = try channelFactory.incomingChannel(with: peer, ownKeyId: ownKeyId.toMessageExchangeOwn())
        let proofVerifier = StatementPermissiveProofVerifier()
        let rawTopic = try topic.fixedStatementFieldData()

        return StatementSubscription(
            connection: statementStoreConnection,
            topicFilter: .matchAll([rawTopic]),
            proofVerifier: proofVerifier,
            workQueue: pollDispatchQueue,
            logger: logger
        )
    }

    func handleStatement(
        _ statement: Statement,
        ownKeyId: Chat.Contact.Own
    ) async throws -> ChatRequest.ValidatedRemoteModel {
        guard let scaleEncodedPayload = statement.getScaleEncodedPayload() else {
            logger.warning("Statement missing payload")
            throw IncomingChatRequestServiceError.noPayload
        }

        let payloadDecoder = try ScaleDecoder(data: scaleEncodedPayload)
        let payload = try Data(scaleDecoder: payloadDecoder)

        return try await chatRequestFactory.decodeAndValidate(remotePayload: payload, ownKeyId: ownKeyId)
    }
}
