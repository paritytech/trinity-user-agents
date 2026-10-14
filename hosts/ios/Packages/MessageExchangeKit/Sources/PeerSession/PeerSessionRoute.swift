import Foundation
import StatementStore

public enum PeerSessionRoute: Hashable, Sendable {
    case device
    case identity

    struct Context {
        let sessionId: MessageExchange.SessionId
        let requestChannelId: StatementFixedFieldConvertible
        let responseChannelId: StatementFixedFieldConvertible
        let peerRequestChannelId: StatementFixedFieldConvertible
    }

    struct Contexts {
        let device: Context
        let identity: Context

        func context(for route: PeerSessionRoute) -> Context {
            switch route {
            case .device:
                device
            case .identity:
                identity
            }
        }
    }
}

public final class PeerSessionRouteSelector<M: MessageExchange.CodableMessage> {
    private let selectRoute: (M) -> PeerSessionRoute

    public init(selectRoute: @escaping (M) -> PeerSessionRoute) {
        self.selectRoute = selectRoute
    }

    func route(for message: M) -> PeerSessionRoute {
        selectRoute(message)
    }
}
