import Foundation
import Foundation_iOS
import MessageExchangeKit

extension SSOTruAPICoordinator: PeerSessionDelegate, TypeErasedDelegateStoring {
    typealias Message = OpaqueSSORawHostMessage

    func peerSession(
        _: any PeerSessionProtocol,
        didUpdateState _: PeerSessionState
    ) {}

    func peerSession(
        _: any PeerSessionProtocol,
        didInitializeWithOutgoingMessages messages: [OpaqueSSORawHostMessage]
    ) {
        let retainedIds = Set(messages.map(\.message.messageId))

        Task {
            await handleSessionReinitialized(retainedMessageIds: retainedIds)
        }
    }

    func peerSession(
        _: any PeerSessionProtocol,
        shouldResetAfter _: MessageExchange.InitializationError
    ) -> Bool { true }

    func peerSession(
        _: any PeerSessionProtocol,
        didFinishAddingMessageToQueue message: OpaqueSSORawHostMessage,
        withError error: MessageExchange.AddToQueueError?
    ) {
        guard let error else { return }

        Task {
            await handleDidPostMessages([message], withError: error)
        }
    }

    func peerSession(
        _: any PeerSessionProtocol,
        didPostMessages messages: [OpaqueSSORawHostMessage],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        Task {
            await handleDidPostMessages(messages, withError: error)
        }
    }

    func peerSession(
        _: any PeerSessionProtocol,
        didDeliverMessages _: [OpaqueSSORawHostMessage],
        withError _: MessageExchange.OutgoingMessageError?
    ) {}

    func peerSession(
        _ session: any PeerSessionProtocol,
        didReceiveMessages messages: [OpaqueSSORawHostMessage],
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        Task {
            await handleIncomingMessages(
                messages,
                from: session.peer,
                completion: respondHandler
            )
        }
    }

    func peerSessionDidReceiveMessagesError(
        _: any PeerSessionProtocol,
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        respondHandler(.success)
    }

    func peerSession(
        _: any PeerSessionProtocol,
        shouldIgnoreStatementAfter _: MessageExchange.IncomingMessageError
    ) -> Bool { true }

    func peerSession(
        _: any PeerSessionProtocol,
        shouldReinitializeAfterSubmitError _: any Error
    ) -> Bool { true }

    func peerSession(
        _: any PeerSessionProtocol,
        didCompactMessages _: OpaqueSSORawHostMessage,
        originalMessages _: [OpaqueSSORawHostMessage]
    ) {}
}
