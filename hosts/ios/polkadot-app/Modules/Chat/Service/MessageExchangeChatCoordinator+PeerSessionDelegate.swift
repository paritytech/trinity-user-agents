import Foundation
import Foundation_iOS
import MessageExchangeKit

extension MessageExchangeChatCoordinator: PeerSessionDelegate, TypeErasedDelegateStoring {
    typealias Message = Chat.OpaqueMessage

    func peerSession(
        _: any PeerSessionProtocol,
        didUpdateState state: PeerSessionState
    ) {
        logger.debug("Did change session state to \(state)")

        if case .active = state {
            outboxService.schedulePendingMessages()
        }
    }

    func peerSession(
        _ session: any PeerSessionProtocol,
        didInitializeWithOutgoingMessages messages: [Message]
    ) {
        let remoteMessages = messages.map(\.remoteMessage)
        handleSyncRemotePendingMessages(remoteMessages, for: session.peer)
    }

    func peerSession(
        _: any PeerSessionProtocol,
        shouldResetAfter _: MessageExchange.InitializationError
    ) -> Bool {
        // if statement decoding failed:
        //       notify user to update the app/contact support
        //   else if statement data decryption failed:
        //       if Contact already up to date:
        //           allow session to proceed (forget about broken statements)
        //       else:
        //           update Contact public key
        //           recreate session
        //   else if statement data decoding failed:
        //       allow session to proceed (make sure messages are resent)
        true
    }

    func peerSession(
        _: any PeerSessionProtocol,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    ) {
        if let error {
            logger.error("Message \(message.remoteMessage.messageId) add to queue error: \(error)")
        } else {
            logger.debug("Message \(message.remoteMessage.messageId) added to queue")
        }
    }

    func peerSession(
        _ session: any PeerSessionProtocol,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        let remoteMessages = messages.map(\.remoteMessage)
        handleSentMessages(
            remoteMessages,
            to: session.peer,
            withError: error
        )
    }

    func peerSession(
        _ session: any PeerSessionProtocol,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        let remoteMessages = messages.map(\.remoteMessage)
        handleDeliveredMessages(
            remoteMessages,
            to: session.peer,
            withError: error
        )
    }

    func peerSession(
        _ session: any PeerSessionProtocol,
        didReceiveMessages messages: [Message],
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        let remoteMessages = messages.map(\.remoteMessage)
        handleIncomingMessages(
            remoteMessages,
            from: session.peer,
            completion: respondHandler
        )
    }

    func peerSessionDidReceiveMessagesError(
        _: any PeerSessionProtocol,
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        // TODO: Consider to remove before release or when testnet change
        // Ignore whole batch if there were an error during decoding

        respondHandler(.success)
    }

    func peerSession(
        _: any PeerSessionProtocol,
        shouldIgnoreStatementAfter _: MessageExchange.IncomingMessageError
    ) -> Bool {
//        if statement decoding failed:
//                no action (should be just ignored by TransportLayer)
//            else if statement data decryption failed:
//                if Contact already up to date:
//                    allow session to proceed (forget about broken statements)
//                else:
//                    update Contact public key
//                    recreate session
//            else if statement data decoding failed:
//                allow session to proceed
        true
    }

    func peerSession(
        _: any PeerSessionProtocol,
        shouldReinitializeAfterSubmitError _: Error
    ) -> Bool { true }

    func peerSession(
        _ session: any PeerSessionProtocol,
        didCompactMessages compactedMessage: Message,
        originalMessages: [Message]
    ) {
        let compactedRemote = compactedMessage.remoteMessage
        let originalRemotes = originalMessages.map(\.remoteMessage)
        handleCompactedMessages(
            compactedMessage: compactedRemote,
            originalMessages: originalRemotes,
            for: session.peer
        )
    }
}
