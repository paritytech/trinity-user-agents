@testable import polkadot_app
import Foundation
import Foundation_iOS
import MessageExchangeKit
import CryptoKit

final class MockChatParticipant: TypeErasedDelegateStoring {
    typealias Message = Chat.OpaqueMessage

    var didUpdateStateClosure: ((
        any PeerSessionProtocol,
        PeerSessionState
    ) -> Void)?

    var didInitializeClosure: ((
        any PeerSessionProtocol,
        [Message]
    ) -> Void)?

    var shouldResetAfterInitializationErrorClosure: ((
        any PeerSessionProtocol,
        MessageExchange.InitializationError
    ) -> Bool)?

    var didFinishAddingMessageToQueueClosure: ((
        any PeerSessionProtocol,
        Message,
        MessageExchange.AddToQueueError?
    ) -> Void)?

    var didPostMessagesClosure: ((
        any PeerSessionProtocol,
        [Message],
        MessageExchange.OutgoingMessageError?
    ) -> Void)?

    var didCompactMessagesClosure: ((
        any PeerSessionProtocol,
        Message,
        [Message]
    ) -> Void)?

    var didDeliverMessagesClosure: ((
        any PeerSessionProtocol,
        [Message],
        MessageExchange.OutgoingMessageError?
    ) -> Void)?

    var didReceiveMessagesClosure: ((
        any PeerSessionProtocol,
        [Message],
        @escaping (MessageExchange.ResponseCode) -> Void
    ) -> Void)?

    var shouldIgnoreStatementClosure: ((
        any PeerSessionProtocol,
        MessageExchange.IncomingMessageError
    ) -> Bool)?

    var shouldReinitializeClosure: ((
        any PeerSessionProtocol,
        Error
    ) -> Bool)?
}

extension MockChatParticipant: PeerSessionDelegate {
    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didCompactMessages compactedMessage: Chat.OpaqueMessage,
        originalMessages: [Chat.OpaqueMessage]
    ) {
        didCompactMessagesClosure?(peerSession, compactedMessage, originalMessages)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didUpdateState state: PeerSessionState
    ) {
        didUpdateStateClosure?(peerSession, state)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didInitializeWithOutgoingMessages messages: [Message]
    ) {
        didInitializeClosure?(peerSession, messages)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldResetAfter error: MessageExchange.InitializationError
    ) -> Bool {
        shouldResetAfterInitializationErrorClosure?(peerSession, error) ?? false
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    ) {
        didFinishAddingMessageToQueueClosure?(peerSession, message, error)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        didPostMessagesClosure?(peerSession, messages, error)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        didDeliverMessagesClosure?(peerSession, messages, error)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didReceiveMessages messages: [Message],
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        didReceiveMessagesClosure?(peerSession, messages, respondHandler)
    }

    func peerSessionDidReceiveMessagesError(
        _ peerSession: any PeerSessionProtocol,
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        didReceiveMessagesClosure?(peerSession, [], respondHandler)
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldIgnoreStatementAfter error: MessageExchange.IncomingMessageError
    ) -> Bool {
        shouldIgnoreStatementClosure?(peerSession, error) ?? true
    }

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldReinitializeAfterSubmitError error: Error
    ) -> Bool {
        shouldReinitializeClosure?(peerSession, error) ?? false
    }
}
