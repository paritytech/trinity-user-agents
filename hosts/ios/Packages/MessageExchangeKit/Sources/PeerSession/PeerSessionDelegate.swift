import Foundation
import Foundation_iOS

public protocol PeerSessionDelegate: AnyObject {
    associatedtype Message: MessageExchange.CodableMessage

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didUpdateState state: PeerSessionState
    )

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didInitializeWithOutgoingMessages messages: [Message]
    )

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldResetAfter error: MessageExchange.InitializationError
    ) -> Bool

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    )

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    )

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    )

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didReceiveMessages messages: [Message],
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    )

    func peerSessionDidReceiveMessagesError(
        _ peerSession: any PeerSessionProtocol,
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    )

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldIgnoreStatementAfter error: MessageExchange.IncomingMessageError
    ) -> Bool

    /// Called when the transport surfaces a submit error that is not handled
    /// automatically by the session. In particular, `StatementSubmitError.rejected(.channelPriorityTooLow)`
    /// is delegated here so the app can decide whether to reinitialize: it
    /// commonly happens during routine bursts of outgoing requests and does
    /// not necessarily warrant tearing the session down.
    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldReinitializeAfterSubmitError error: Error
    ) -> Bool

    func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didCompactMessages compactedMessage: Message,
        originalMessages: [Message]
    )
}

// MARK: - Type Erasure Implementation

public final class AnyPeerSessionDelegate<M: MessageExchange.CodableMessage>: PeerSessionDelegate {
    public typealias Message = M

    private let didUpdateStateClosure: (
        any PeerSessionProtocol,
        PeerSessionState
    ) -> Void

    private let didInitializeClosure: (
        any PeerSessionProtocol,
        [M]
    ) -> Void

    private let shouldResetClosure: (
        any PeerSessionProtocol,
        MessageExchange.InitializationError
    ) -> Bool

    private let didFinishAddingMessageToQueue: (
        any PeerSessionProtocol,
        M,
        MessageExchange.AddToQueueError?
    ) -> Void

    private let didPostMessagesClosure: (
        any PeerSessionProtocol,
        [M],
        MessageExchange.OutgoingMessageError?
    ) -> Void

    private let didDeliverMessagesClosure: (
        any PeerSessionProtocol,
        [M],
        MessageExchange.OutgoingMessageError?
    ) -> Void

    private let didReceiveMessagesClosure: (
        any PeerSessionProtocol,
        [M],
        @escaping (MessageExchange.ResponseCode) -> Void
    ) -> Void

    private let didReceiveMessagesErrorClosure: (
        any PeerSessionProtocol,
        @escaping (MessageExchange.ResponseCode) -> Void
    ) -> Void

    private let shouldIgnoreStatementClosure: (
        any PeerSessionProtocol,
        MessageExchange.IncomingMessageError
    ) -> Bool

    private let shouldReinitializeClosure: (
        any PeerSessionProtocol,
        Error
    ) -> Bool

    private let didCompactMessagesClosure: (
        any PeerSessionProtocol,
        M,
        [M]
    ) -> Void

    public init<
        D: PeerSessionDelegate & TypeErasedDelegateStoring
    >(_ targetDelegate: D) where D.Message == M {
        didUpdateStateClosure = { [weak targetDelegate] peerSession, state in
            targetDelegate?.peerSession(peerSession, didUpdateState: state)
        }

        didInitializeClosure = { [weak targetDelegate] peerSession, messages in
            targetDelegate?.peerSession(
                peerSession,
                didInitializeWithOutgoingMessages: messages
            )
        }

        shouldResetClosure = { [weak targetDelegate] peerSession, error in
            targetDelegate?.peerSession(
                peerSession,
                shouldResetAfter: error
            ) ?? MessageExchange.shouldResetSession
        }

        didFinishAddingMessageToQueue = { [weak targetDelegate] peerSession, message, error in
            targetDelegate?.peerSession(
                peerSession,
                didFinishAddingMessageToQueue: message,
                withError: error
            )
        }

        didPostMessagesClosure = { [weak targetDelegate] peerSession, messages, error in
            targetDelegate?.peerSession(
                peerSession,
                didPostMessages: messages,
                withError: error
            )
        }

        didDeliverMessagesClosure = { [weak targetDelegate] peerSession, messages, error in
            targetDelegate?.peerSession(
                peerSession,
                didDeliverMessages: messages,
                withError: error
            )
        }

        didReceiveMessagesClosure = { [weak targetDelegate] peerSession, messages, respondHandler in
            targetDelegate?.peerSession(
                peerSession,
                didReceiveMessages: messages,
                respondHandler: respondHandler
            )
        }

        didReceiveMessagesErrorClosure = { [weak targetDelegate] peerSession, respondHandler in
            targetDelegate?.peerSessionDidReceiveMessagesError(
                peerSession,
                respondHandler: respondHandler
            )
        }

        shouldIgnoreStatementClosure = { [weak targetDelegate] peerSession, error in
            targetDelegate?.peerSession(
                peerSession,
                shouldIgnoreStatementAfter: error
            ) ?? MessageExchange.shouldIgnoreStatement
        }

        shouldReinitializeClosure = { [weak targetDelegate] peerSession, error in
            targetDelegate?.peerSession(
                peerSession,
                shouldReinitializeAfterSubmitError: error
            ) ?? MessageExchange.shouldReinitializeSession
        }

        didCompactMessagesClosure = { [weak targetDelegate] peerSession, compactedMessage, originalMessages in
            targetDelegate?.peerSession(
                peerSession,
                didCompactMessages: compactedMessage,
                originalMessages: originalMessages
            )
        }

        targetDelegate.storeErasedType(instance: self)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didUpdateState state: PeerSessionState
    ) {
        didUpdateStateClosure(peerSession, state)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didInitializeWithOutgoingMessages messages: [M]
    ) {
        didInitializeClosure(peerSession, messages)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldResetAfter error: MessageExchange.InitializationError
    ) -> Bool {
        shouldResetClosure(peerSession, error)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didFinishAddingMessageToQueue message: M,
        withError error: MessageExchange.AddToQueueError?
    ) {
        didFinishAddingMessageToQueue(peerSession, message, error)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        didPostMessagesClosure(peerSession, messages, error)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        didDeliverMessagesClosure(peerSession, messages, error)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didReceiveMessages messages: [Message],
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        didReceiveMessagesClosure(peerSession, messages, respondHandler)
    }

    public func peerSessionDidReceiveMessagesError(
        _ peerSession: any PeerSessionProtocol,
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        didReceiveMessagesErrorClosure(peerSession, respondHandler)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldIgnoreStatementAfter error: MessageExchange.IncomingMessageError
    ) -> Bool {
        shouldIgnoreStatementClosure(peerSession, error)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        shouldReinitializeAfterSubmitError error: Error
    ) -> Bool {
        shouldReinitializeClosure(peerSession, error)
    }

    public func peerSession(
        _ peerSession: any PeerSessionProtocol,
        didCompactMessages compactedMessage: M,
        originalMessages: [M]
    ) {
        didCompactMessagesClosure(peerSession, compactedMessage, originalMessages)
    }
}
