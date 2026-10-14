import Foundation
import StatementStore

protocol OutgoingMessageChanneling {
    associatedtype Message: MessageExchange.CodableMessage

    func activate(restoringState requests: [PeerSessionRoute: OutgoingRequest<Message>])
    func deactivate()
    func addMessagesToQueue(_ messages: [Message])
    func handleResponse(
        _ response: MessageExchange.Response,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus
    func reset(route: PeerSessionRoute)
}

// MARK: - Type Erasure Implementation

final class AnyOutgoingMessageChannel<M: MessageExchange.CodableMessage>: OutgoingMessageChanneling {
    typealias Message = M

    private let activateClosure: ([PeerSessionRoute: OutgoingRequest<Message>]) -> Void
    private let deactivateClosure: () -> Void
    private let addMessagesToQueueClosure: ([Message]) -> Void
    private let handleResponseClosure: (MessageExchange.Response, PeerSessionRoute) -> StatementHandlingStatus
    private let resetClosure: (PeerSessionRoute) -> Void

    init<Channel: OutgoingMessageChanneling>(_ targetChannel: Channel) where Channel.Message == M {
        activateClosure = { requests in
            targetChannel.activate(restoringState: requests)
        }

        deactivateClosure = {
            targetChannel.deactivate()
        }

        addMessagesToQueueClosure = { messages in
            targetChannel.addMessagesToQueue(messages)
        }

        handleResponseClosure = { response, route in
            targetChannel.handleResponse(response, route: route)
        }

        resetClosure = { route in
            targetChannel.reset(route: route)
        }
    }

    func activate(restoringState requests: [PeerSessionRoute: OutgoingRequest<Message>]) {
        activateClosure(requests)
    }

    func deactivate() {
        deactivateClosure()
    }

    func addMessagesToQueue(_ messages: [M]) {
        addMessagesToQueueClosure(messages)
    }

    func handleResponse(
        _ response: MessageExchange.Response,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        handleResponseClosure(response, route)
    }

    func reset(route: PeerSessionRoute) {
        resetClosure(route)
    }
}
