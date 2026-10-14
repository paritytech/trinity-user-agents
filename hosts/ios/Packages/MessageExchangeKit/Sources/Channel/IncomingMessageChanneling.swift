import Foundation

protocol IncomingMessageChanneling {
    associatedtype Message: MessageExchange.CodableMessage

    func sendResponse(
        with responseCode: MessageExchange.ResponseCode,
        forRequestId requestId: String,
        route: PeerSessionRoute
    )
}

// MARK: - Type Erasure Implementation

final class AnyIncomingMessageChannel<M: MessageExchange.CodableMessage>: IncomingMessageChanneling {
    typealias Message = M

    private let sendResponseClosure: (
        MessageExchange.ResponseCode,
        String,
        PeerSessionRoute
    ) -> Void

    init<Channel: IncomingMessageChanneling>(_ targetChannel: Channel) where Channel.Message == M {
        sendResponseClosure = { code, requestId, route in
            targetChannel.sendResponse(with: code, forRequestId: requestId, route: route)
        }
    }

    func sendResponse(
        with responseCode: MessageExchange.ResponseCode,
        forRequestId requestId: String,
        route: PeerSessionRoute
    ) {
        sendResponseClosure(responseCode, requestId, route)
    }
}
