import Foundation
@preconcurrency import WebRTC

enum WebRTCPeerConnectionFactoryProvider {
    // RTCInitializeSSL must be called exactly once per process. Guarded with
    // dispatch_once semantics so concurrent callers from different WebRTC
    // stacks (call, game, device sync) do not race on it.
    private static let sslInitialization: Void = {
        RTCInitializeSSL()
        RTCSetMinDebugLogLevel(.none)
    }()

    static func make() -> RTCPeerConnectionFactory {
        _ = sslInitialization
        let encoderFactory = RTCDefaultVideoEncoderFactory()
        let decoderFactory = RTCDefaultVideoDecoderFactory()
        return RTCPeerConnectionFactory(
            encoderFactory: encoderFactory,
            decoderFactory: decoderFactory
        )
    }
}
