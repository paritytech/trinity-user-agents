import Foundation
import WebRTC

enum PeerDataConnectionState {
    struct Connected {
        let connection: AsyncPeerConnectionWrapper
        let dataChannel: AsyncDataChannelWrapper
    }

    case waiting
    case connecting
    case connected(Connected)
    case disconnected
}
