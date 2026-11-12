import Foundation
import SubstrateSdk
import MessageExchangeKit

actor OutgoingChatRequestCoordinationContext {
    enum MessageState {
        case sending(Task<Void, Never>)
        case sent

        var isSent: Bool {
            switch self {
            case .sent:
                true
            case .sending:
                false
            }
        }
    }

    nonisolated let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }

    private var peersWithOutgoingRequests: [AccountId: MessageExchange.SessionRequest] = [:]
    private var messageState: [String: MessageState] = [:]
    private var sentMessages: Set<String> = []
    private var outgoingRequestsTask: Task<Void, Never>?

    func update(
        contacts: [Chat.Contact],
        outgoingRequestTaskBuilder: () -> Task<Void, Never>
    ) {
        let newPeers: [AccountId: MessageExchange.SessionRequest] = contacts.reduce(into: [:]) { accum, contact in
            guard contact.hasOutgoingChatRequest else { return }

            accum[contact.accountId] = MessageExchange.SessionRequest(
                own: contact.ownKeyId.toMessageExchangeOwn(),
                peer: contact.toMessageExchangePeer()
            )
        }

        guard newPeers != peersWithOutgoingRequests else {
            return
        }

        peersWithOutgoingRequests = newPeers

        outgoingRequestsTask?.cancel()
        outgoingRequestsTask = outgoingRequestTaskBuilder()

        // don't process already sent messages again in any case
        messageState = messageState.filter { !$0.value.isSent }
    }

    func process(
        requestMessages: [Chat.LocalMessage],
        deliver: @escaping @Sendable (Chat.RequestMessage, MessageExchange.SessionRequest) async -> Void
    ) {
        let nonSendingMessages = requestMessages.filter { messageState[$0.messageId] == nil }

        nonSendingMessages.forEach { message in
            guard case let .person(accountId) = message.chatId else {
                logger.error("Not a message to person")
                return
            }

            guard let sessionRequest = peersWithOutgoingRequests[accountId] else {
                logger.error("No peer found for accountId: \(accountId.toHex())")
                return
            }

            guard let remoteMessage = Chat.RequestMessage(localMessage: message) else {
                logger.error("Not a request message, content: \(message.content)")
                return
            }

            let task = Task { [weak self] in
                self?.logger.debug("Delivering request: \(remoteMessage.messageId)")
                await deliver(remoteMessage, sessionRequest)
                await self?.finishDelivery(of: remoteMessage.messageId)
            }

            messageState[remoteMessage.messageId] = .sending(task)
        }
    }
}

extension OutgoingChatRequestCoordinationContext {
    func cancelAll() {
        outgoingRequestsTask?.cancel()
        outgoingRequestsTask = nil

        for case let .sending(task) in messageState.values {
            task.cancel()
        }

        messageState = [:]
        peersWithOutgoingRequests = [:]
    }
}

private extension OutgoingChatRequestCoordinationContext {
    // A cancelled delivery is forgotten, so the next run picks the still unsent message up again.
    func finishDelivery(of messageId: String) {
        messageState[messageId] = Task.isCancelled ? nil : .sent
    }
}
