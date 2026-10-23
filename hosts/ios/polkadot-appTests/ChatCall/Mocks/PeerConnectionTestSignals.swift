import Foundation

@testable import polkadot_app

/// Signals of a data connection, as the WebRTC stack of a call produces them.
enum PeerConnectionTestSignals {
    static let validAnswerSdp = """
    v=0
    o=- 9876543210 1 IN IP4 0.0.0.0
    s=-
    t=0 0
    m=application 9 UDP/DTLS/SCTP webrtc-datachannel
    c=IN IP4 0.0.0.0
    a=ice-ufrag:answerUfrag
    a=ice-pwd:answerPassword456
    a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
    a=setup:active
    a=mid:0
    a=sctp-port:5000

    """

    static func makeIPv4Candidate(
        foundation: String = "1",
        priority: UInt32 = 2_130_706_431,
        transport: String = "UDP",
        ip: String = "192.168.1.100",
        port: UInt16 = 54_321,
        type: String = "host"
    ) -> PeerConnectionCandidate {
        let sdp = "candidate:\(foundation) 1 \(transport) \(priority) \(ip) \(port) typ \(type)"
        return PeerConnectionCandidate(sdp: sdp, sdpMLineIndex: 0, sdpMid: "0")
    }
}
