import Foundation
import SubstrateSdk
import MessageExchangeKit

final class ChatRequestAcceptProcessor {
    let context: ChatRequestAcceptProcessorContext
    let messageExchangeModeProvider: MessageExchangeModeProviding
    let logger: LoggerProtocol

    init(
        messageExchangeModeProvider: MessageExchangeModeProviding,
        requestStoreService: ChatRequestStoreServicing,
        messageStoreService: MessagesLocalStorageServicing,
        contactsStorageService: ContactsLocalStorageServicing,
        logger: LoggerProtocol
    ) {
        context = ChatRequestAcceptProcessorContext(
            requestStoreService: requestStoreService,
            messageStoreService: messageStoreService,
            contactsStorageService: contactsStorageService,
            logger: logger
        )

        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.logger = logger
    }
}

extension ChatRequestAcceptProcessor: IncomingMessageProcessing {
    // Mark outgoing request as accepted when either:
    // - there is incoming chatAccepted message with matching id. We also mark corresponding request message
    // as delivered if incoming message's timestamp exceeds the request timestamp;
    // - there is previous contact added message and there is no left chat message after it (to be compatible with
    // previous spec);
    // - there is an incoming message with timestamp greater than one in the request. That means peer already added us
    // to the contact list and started sending messages;
    // The first case is handled by general handleRequest function and 2 latter cases by handleCurrentRequest
    func process(messages: [Chat.RemoteMessage], from contact: Chat.Contact) {
        Task {
            let handledCurrentRequest = await handleRequests(to: contact, messages: messages)

            if !handledCurrentRequest, let request = contact.chatRequest, request.isOutgoing {
                await handleCurrentRequest(
                    request,
                    messages: messages,
                    messageExchangeMode: messageExchangeModeProvider.mode(for: contact)
                )
            }
        }
    }
}

private extension ChatRequestAcceptProcessor {
    // Accepts outgoing requests if there is matching incoming message.
    // We also mark corresponding request message
    // as delivered if incoming message's timestamp exceeds the request timestamp.
    // Returns true if current request to the contact is handled otherwise returns false.
    func handleRequests(to contact: Chat.Contact, messages: [Chat.RemoteMessage]) async -> Bool {
        var handledCurrentRequest = false

        for message in messages {
            do {
                let content = message.versioned.ensureV1()?.content

                var acceptRequestId: String?
                var peerDevice: Chat.PeerDevice?

                if case let .chatAccepted(model) = content {
                    acceptRequestId = model.messageId
                } else if case let .multiChatAccepted(model) = content {
                    acceptRequestId = model.requestId
                    peerDevice = model.device
                }

                guard let acceptRequestId else { continue }

                logger.debug("Handling accept for request with id: \(acceptRequestId)")

                let messageExchangeMode = messageExchangeModeProvider.mode(for: contact)

                let request = try await context.accept(
                    requestId: acceptRequestId,
                    from: contact.accountId,
                    messageExchangeMode: messageExchangeMode
                )

                if let peerDevice {
                    switch messageExchangeMode {
                    case .identity:
                        break
                    case .multidevice:
                        logger.debug("Storing peer device for contact")
                        await context.storePeerDevices([peerDevice], for: contact.accountId)
                    }
                }

                if let request, request.shouldMarkMessageAsDelivered(for: message) {
                    logger.debug("Marking message as delivered")
                    try await context.markMessageAsDelivered(for: request.requestId)
                    logger.debug("Marked message as delivered")
                }

                if let request, request.requestId == contact.chatRequest?.requestId {
                    handledCurrentRequest = true
                }
            } catch {
                logger.error("Handling accept for old request failed: \(error)")
            }
        }

        return handledCurrentRequest
    }

    func handleCurrentRequest(
        _ request: Chat.Request,
        messages: [Chat.RemoteMessage],
        messageExchangeMode: MessageExchangeMode
    ) async {
        let isResolved = await resolveOutgoingRequestFromContactAdded(
            request: request,
            messages: messages,
            messageExchangeMode: messageExchangeMode
        )

        if !isResolved {
            await resolveOutgoingRequestWithLaterMessage(
                request: request,
                messages: messages,
                messageExchangeMode: messageExchangeMode
            )
        }
    }

    // Consider request as accepted if there is previous contact added message
    // not followed by left chat message
    @discardableResult
    func resolveOutgoingRequestFromContactAdded(
        request: Chat.Request,
        messages: [Chat.RemoteMessage],
        messageExchangeMode: MessageExchangeMode
    ) async -> Bool {
        let sortedMessages = messages.sorted { $0.timestamp >= $1.timestamp }

        let optContactAddedMessage = sortedMessages.first { message in
            message.versioned.ensureV1()?.content == .contactAdded
        }

        guard
            let contactAddedMessage = optContactAddedMessage,
            contactAddedMessage.timestamp <= request.timestamp else {
            return false
        }

        let optLeftChatMessage = sortedMessages.first { message in
            message.versioned.ensureV1()?.content == .leftChat
        }

        if
            let leftChatMessage = optLeftChatMessage,
            leftChatMessage.timestamp >= contactAddedMessage.timestamp {
            return false
        }

        do {
            logger.debug("Accepting request after contact added with id: \(request.requestId)")

            try await context.accept(
                requestId: request.requestId,
                from: request.contactAccountId,
                messageExchangeMode: messageExchangeMode
            )
            return true
        } catch {
            logger.debug("Accepting request after contact added failed: \(error)")
            return false
        }
    }

    @discardableResult
    func resolveOutgoingRequestWithLaterMessage(
        request: Chat.Request,
        messages: [Chat.RemoteMessage],
        messageExchangeMode: MessageExchangeMode
    ) async -> Bool {
        let laterMessage = messages.first { request.shouldMarkAsAccepted(for: $0) }
        guard laterMessage != nil else {
            return false
        }

        do {
            logger.debug("Accepting request after newest message: \(request.requestId)")

            try await context.accept(
                requestId: request.requestId,
                from: request.contactAccountId,
                messageExchangeMode: messageExchangeMode
            )
            return true
        } catch {
            logger.debug("Accepting request after newest message failed: \(error)")
            return false
        }
    }
}

private extension Chat.Request {
    func shouldMarkMessageAsDelivered(for incomingMessage: Chat.RemoteMessage) -> Bool {
        let content = incomingMessage.versioned.ensureV1()?.content

        let matchingRequestId: String? =
            switch content {
            case let .chatAccepted(model):
                model.messageId
            case let .multiChatAccepted(model):
                model.requestId
            default:
                nil
            }

        guard let matchingRequestId else {
            return false
        }

        guard let message, case let .outgoing(outgoingStatus) = message.status else {
            return false
        }

        return outgoingStatus != .delivered &&
            requestId == matchingRequestId &&
            incomingMessage.timestamp >= timestamp
    }

    func shouldMarkAsAccepted(for incomingMessage: Chat.RemoteMessage) -> Bool {
        isOutgoing && incomingMessage.timestamp >= timestamp
    }
}
