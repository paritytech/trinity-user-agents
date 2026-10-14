import Foundation

extension Chat {
    struct PushId {
        let own: Data
        let peer: Data

        var ownString: String {
            own.toHex()
        }

        var peerString: String {
            peer.toHex()
        }
    }
}
