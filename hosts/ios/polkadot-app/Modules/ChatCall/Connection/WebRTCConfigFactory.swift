import Foundation
import WebRTC

protocol WebRTCConfigMaking: Sendable {
    func makeConnectionConfiguration() async throws -> RTCConfiguration
    func makeDataChannelConfiguration() -> RTCDataChannelConfiguration
}

final class WebRTCConfigFactory: WebRTCConfigMaking {
    private let turnService: TURNCredentialsProviding
    private let iceCandidatePoolSize: Int32
    private let allowsDirectFallback: Bool

    init(
        turnService: TURNCredentialsProviding,
        iceCandidatePoolSize: Int32 = 8,
        allowsDirectFallback: Bool = false
    ) {
        self.turnService = turnService
        self.iceCandidatePoolSize = iceCandidatePoolSize
        self.allowsDirectFallback = allowsDirectFallback
    }

    func makeConnectionConfiguration() async throws -> RTCConfiguration {
        try Task.checkCancellation()
        let iceServers: [RTCIceServer]
        do {
            iceServers = makeIceServers(from: try await turnService.issueCredentials())
        } catch {
            try Task.checkCancellation()
            if error is CancellationError || (error as? URLError)?.code == .cancelled {
                throw error
            }
            guard allowsDirectFallback else { throw error }
            // Media can connect directly when the optional TURN service is unavailable.
            iceServers = []
        }
        try Task.checkCancellation()

        let configuration = RTCConfiguration()
        configuration.iceServers = iceServers
        configuration.sdpSemantics = .unifiedPlan
        configuration.iceCandidatePoolSize = iceCandidatePoolSize
        // recommended by WebRTC team to avoid complexity explosion
        configuration.maxIPv6Networks = 1

        return configuration
    }

    func makeDataChannelConfiguration() -> RTCDataChannelConfiguration {
        let dataChannelConfig = RTCDataChannelConfiguration()
        dataChannelConfig.isOrdered = true
        dataChannelConfig.maxRetransmits = 3
        dataChannelConfig.isNegotiated = false

        return dataChannelConfig
    }
}

private extension WebRTCConfigFactory {
    func makeIceServers(from credentials: TURNCredentials) -> [RTCIceServer] {
        var result: [RTCIceServer] = []

        if !credentials.stunUrls.isEmpty {
            result.append(RTCIceServer(urlStrings: credentials.stunUrls))
        }

        if !credentials.turnUrls.isEmpty {
            result.append(
                RTCIceServer(
                    urlStrings: credentials.turnUrls,
                    username: credentials.username,
                    credential: credentials.password
                )
            )
        }

        return result
    }
}
