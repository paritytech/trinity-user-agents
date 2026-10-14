import WebRTC

extension RTCIceConnectionState {
    var isTerminal: Bool {
        self == .failed || self == .closed
    }
}
