import Foundation
import Foundation_iOS
import MessageExchangeKit

extension MessageExchangeSignInHostCoordinator: PeerSessionDelegate, TypeErasedDelegateStoring {
    typealias Message = OpaquePolkadotHostRemoteMessage

    func peerSession(
        _: any PeerSessionProtocol,
        didUpdateState _: PeerSessionState
    ) {}

    func peerSession(
        _: any PeerSessionProtocol,
        didInitializeWithOutgoingMessages messages: [OpaquePolkadotHostRemoteMessage]
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
        didFinishAddingMessageToQueue message: OpaquePolkadotHostRemoteMessage,
        withError error: MessageExchange.AddToQueueError?
    ) {
        guard let error else {
            return
        }

        Task {
            await handleDidPostMessages(
                [message.message],
                withError: error
            )
        }
    }

    func peerSession(
        _: any PeerSessionProtocol,
        didPostMessages messages: [OpaquePolkadotHostRemoteMessage],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        Task {
            await handleDidPostMessages(
                messages.map(\.message),
                withError: error
            )
        }
    }

    func peerSession(
        _: any PeerSessionProtocol,
        didDeliverMessages _: [OpaquePolkadotHostRemoteMessage],
        withError _: MessageExchange.OutgoingMessageError?
    ) {}

    func peerSession(
        _ session: any PeerSessionProtocol,
        didReceiveMessages messages: [OpaquePolkadotHostRemoteMessage],
        respondHandler: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        Task {
            await handleIncomingMessages(
                messages.map(\.message),
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
        didCompactMessages _: OpaquePolkadotHostRemoteMessage,
        originalMessages _: [OpaquePolkadotHostRemoteMessage]
    ) {}
}
