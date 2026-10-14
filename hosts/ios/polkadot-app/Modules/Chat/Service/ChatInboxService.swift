import Foundation
import Operation_iOS
import MessageExchangeKit

protocol ChatInboxServicing: AnyObject {
    func handleIncomingMessages(
        messages: [Chat.RemoteMessage],
        from contact: Chat.Contact,
        completion: @escaping (MessageExchange.ResponseCode) -> Void
    )

    func setupCallCoordinator(_ callCoordinator: CallCoordinating)
}

final class ChatInboxService {
    let messagesStorageService: MessagesLocalStorageServicing
    let workQueue: DispatchQueue
    let operationQueue: OperationQueue
    let incomingMessageProcessor: IncomingMessageProcessing
    let logger: LoggerProtocol

    weak var callCoordinator: CallCoordinating?

    init(
        incomingMessageProcessor: IncomingMessageProcessing,
        messagesStorageService: MessagesLocalStorageServicing,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.incomingMessageProcessor = incomingMessageProcessor
        self.messagesStorageService = messagesStorageService
        self.workQueue = workQueue
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension ChatInboxService: ChatInboxServicing {
    func setupCallCoordinator(_ callCoordinator: CallCoordinating) {
        self.callCoordinator = callCoordinator
    }

    func handleIncomingMessages(
        messages: [Chat.RemoteMessage],
        from contact: Chat.Contact,
        completion: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }

            let callMessages = messages.filter(\.isForCallProtocol)

            if !callMessages.isEmpty {
                Task { [weak self] in
                    await self?.handleCallMessages(callMessages, from: contact)
                }
            }

            let localMessages: [Chat.LocalMessage] = messages.compactMap { message in
                guard Chat.LocalMessage.supportsRemote(message) else {
                    return nil
                }

                return Chat.LocalMessage(
                    remote: message,
                    creationSource: .localDevice,
                    status: .incoming(.new),
                    contactId: contact.accountId
                )
            }

            if !localMessages.isEmpty {
                handleLocalMessages(localMessages, completion: completion)
            } else {
                completion(.success)
            }

            incomingMessageProcessor.process(messages: messages, from: contact)
        }
    }
}

private extension ChatInboxService {
    func handleCallMessages(_ messages: [Chat.RemoteMessage], from contact: Chat.Contact) async {
        guard let callCoordinator else {
            logger.error("Call coordinator not set")
            return
        }

        let peer = CallPeer(name: contact.username, accountId: contact.accountId)
        for message in messages {
            await callCoordinator.handleIncomingCall(in: message, from: peer)
        }
    }

    func handleLocalMessages(
        _ messages: [Chat.LocalMessage],
        completion: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        let operation = messagesStorageService.insertOrUpdate(messages)

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: workQueue
        ) { [logger] result in
            switch result {
            case .success:
                logger.info("Incoming \(messages.count) messages processed successfully")
                completion(.success)
            case let .failure(error):
                logger.warning("Failed to process incoming messages: \(error)")
                completion(.invalidMessage)
            }
        }
    }
}

extension MessageExchange.ResponseCode {
    static var invalidMessage: Self {
        .failure(100)
    }
}
