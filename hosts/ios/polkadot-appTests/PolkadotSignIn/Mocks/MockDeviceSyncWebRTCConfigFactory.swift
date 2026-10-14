@testable import polkadot_app
import WebRTC

struct MockDeviceSyncWebRTCConfigFactory: WebRTCConfigMaking {
    func makeConnectionConfiguration() async throws -> RTCConfiguration {
        RTCConfiguration()
    }

    func makeDataChannelConfiguration() -> RTCDataChannelConfiguration {
        RTCDataChannelConfiguration()
    }
}
