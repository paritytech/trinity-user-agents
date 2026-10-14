import Foundation
import WebRTC

struct RTCVideoProfile {
    let captureTargetWidth: Int
    let captureTargetHeight: Int
    let captureFps: Int
    let maxBitrateBps: Int
    let degradationPreference: RTCDegradationPreference

    static let call = RTCVideoProfile(
        captureTargetWidth: 1_280,
        captureTargetHeight: 720,
        captureFps: 30,
        maxBitrateBps: 2_000_000,
        degradationPreference: .balanced
    )

    static let game = RTCVideoProfile(
        captureTargetWidth: 854,
        captureTargetHeight: 480,
        captureFps: 24,
        maxBitrateBps: 600_000,
        degradationPreference: .maintainFramerate
    )
}
