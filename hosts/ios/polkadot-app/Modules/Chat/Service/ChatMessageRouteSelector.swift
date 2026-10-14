import Foundation
import MessageExchangeKit

enum ChatMessageRouteSelector {
    static func makeSelector() -> (Any) -> PeerSessionRoute {
        { message in
            let remoteMessage: Chat.RemoteMessage? =
                switch message {
                case let opaqueMessage as Chat.OpaqueMessage:
                    opaqueMessage.remoteMessage
                case let message as Chat.RemoteMessage:
                    message
                default:
                    nil
                }

            guard let remoteMessage else {
                return .device
            }

            switch remoteMessage.versioned.ensureV1()?.content {
            case .chatAccepted,
                 .multiChatAccepted:
                return .identity
            default:
                return .device
            }
        }
    }
}
