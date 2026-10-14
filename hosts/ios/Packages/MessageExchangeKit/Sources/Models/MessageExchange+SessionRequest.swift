import Foundation

public extension MessageExchange {
    struct SessionRequest: Hashable {
        public let own: Own
        public let peer: Peer

        public init(own: Own, peer: Peer) {
            self.own = own
            self.peer = peer
        }

        public func requiresSessionRecreation(comparedTo other: SessionRequest) -> Bool {
            self != other
        }

        var hasPeerDevices: Bool {
            !peer.devices.isEmpty
        }
    }
}
