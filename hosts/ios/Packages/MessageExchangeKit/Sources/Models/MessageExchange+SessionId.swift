import Foundation

public extension MessageExchange {
    struct SessionId: Hashable {
        public let own: Data
        public let peer: Data
        public let ownParameter: Data
        public let peerParameter: Data
    }
}
