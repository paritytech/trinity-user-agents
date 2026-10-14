import Foundation
import MessageExchangeKit
import StructuredConcurrency
import SubstrateSdk

protocol HostMessageIdentifiable {
    var messageId: String { get }
}

protocol PolkadotHostMessageSending<Message> {
    associatedtype Message: HostMessageIdentifiable & ScaleCodable & Equatable

    func setExchangeService(_ service: AnyMessageExchangeService<OpaqueMessageWrapper<Message>>) async

    func postMessage(
        _ message: Message,
        to host: PolkadotSignInHost
    ) async throws

    func handleDidPostMessages(
        _ messages: [Message],
        withError error: Error?
    ) async

    func cancelPendingMessages(excluding retainedMessageIds: Set<String>) async
}

enum PolkadotHostMessageError: Error {
    case submissionFailed
    case messageTooBig(maxSize: Int, actualSize: Int)
    case deliveryFailed
    case timeout
    case other(Error)
    case serviceExpected

    init(rawPostError: Error) {
        if let submission = rawPostError as? MessageExchange.OutgoingMessageError {
            self.init(submissionError: submission)
        } else if let queueError = rawPostError as? MessageExchange.AddToQueueError {
            self.init(queueError: queueError)
        } else {
            self = .other(rawPostError)
        }
    }

    init(submissionError: MessageExchange.OutgoingMessageError) {
        switch submissionError {
        case .failedToPost:
            self = .submissionFailed
        case .gotFailedResponse:
            self = .deliveryFailed
        }
    }

    init(queueError: MessageExchange.AddToQueueError) {
        switch queueError {
        case let .messageTooBig(maxSize, actualSize):
            self = .messageTooBig(maxSize: maxSize, actualSize: actualSize)
        default:
            self = .other(queueError)
        }
    }
}

actor PolkadotHostMessageSender<Message: HostMessageIdentifiable & ScaleCodable & Equatable> {
    private let postTimeout: Duration
    private let logger: LoggerProtocol

    private var exchangeService: AnyMessageExchangeService<OpaqueMessageWrapper<Message>>?
    private var continuationsByMessageId = [String: CheckedContinuation<Void, Error>]()

    init(
        postTimeout: Duration = .seconds(10),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.postTimeout = postTimeout
        self.logger = logger
    }
}

private extension PolkadotHostMessageSender {
    func enqueueAndWait(
        messageId: String,
        message: Message,
        host: PolkadotSignInHost,
        exchangeService: AnyMessageExchangeService<OpaqueMessageWrapper<Message>>
    ) async throws {
        try await withCheckedThrowingContinuation { continuation in
            continuationsByMessageId[messageId] = continuation

            exchangeService.addMessagesToQueue(
                [OpaqueMessageWrapper(message: message)],
                for: .init(
                    accountId: host.accountId,
                    publicKey: host.publicKey,
                    pin: nil,
                    devices: []
                )
            )

            logger.debug("Added \(messageId) message to queue")
        }
    }
}

extension PolkadotHostMessageSender: PolkadotHostMessageSending {
    func setExchangeService(_ service: AnyMessageExchangeService<OpaqueMessageWrapper<Message>>) async {
        exchangeService = service
    }

    func postMessage(
        _ message: Message,
        to host: PolkadotSignInHost
    ) async throws {
        guard let exchangeService else {
            logger.error("Missing exchange service")

            throw PolkadotHostMessageError.serviceExpected
        }

        let messageId = message.messageId

        do {
            try await withTimeout(postTimeout) { [self] in
                try await enqueueAndWait(
                    messageId: messageId,
                    message: message,
                    host: host,
                    exchangeService: exchangeService
                )
            }
        } catch is TimeoutError {
            continuationsByMessageId.removeValue(forKey: messageId)
            logger.error("Post message \(messageId) timed out")
            throw PolkadotHostMessageError.timeout
        }
    }

    func handleDidPostMessages(
        _ messages: [Message],
        withError error: Error?
    ) async {
        for message in messages {
            guard let continuation = continuationsByMessageId.removeValue(
                forKey: message.messageId
            ) else {
                continue
            }

            if let error {
                logger.error("Failed to post \(message.messageId): \(error)")
                continuation.resume(throwing: PolkadotHostMessageError(rawPostError: error))
            } else {
                logger.debug("Posted \(message.messageId)")
                continuation.resume()
            }
        }
    }

    func cancelPendingMessages(excluding retainedMessageIds: Set<String>) async {
        let orphanedIds = continuationsByMessageId.keys.filter { !retainedMessageIds.contains($0) }

        for messageId in orphanedIds {
            guard let continuation = continuationsByMessageId.removeValue(forKey: messageId) else {
                continue
            }

            logger.warning("Cancelling orphaned message \(messageId)")
            continuation.resume(throwing: PolkadotHostMessageError.submissionFailed)
        }
    }
}
