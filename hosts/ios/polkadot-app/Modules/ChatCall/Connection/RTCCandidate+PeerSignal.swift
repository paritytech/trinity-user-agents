import Foundation
import WebRTC

extension PeerConnectionCandidate {
    init(iceCandidate: RTCIceCandidate) {
        sdp = iceCandidate.sdp
        sdpMLineIndex = UInt32(bitPattern: iceCandidate.sdpMLineIndex)
        sdpMid = iceCandidate.sdpMid
    }

    func toRTCIceCandidate() -> RTCIceCandidate {
        RTCIceCandidate(
            sdp: sdp,
            sdpMLineIndex: Int32(bitPattern: sdpMLineIndex),
            sdpMid: sdpMid
        )
    }
}
