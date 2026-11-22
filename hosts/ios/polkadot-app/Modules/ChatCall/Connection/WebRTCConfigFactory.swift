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

    init(turnService: TURNCredentialsProviding, iceCandidatePoolSize: Int32 = 8, allowsDirectFallback: Bool = false) {
        self.turnService = turnService
        self.iceCandidatePoolSize = iceCandidatePoolSize
        self.allowsDirectFallback = allowsDirectFallback
    }

    func makeConnectionConfiguration() async throws -> RTCConfiguration {
        let credentials: TURNCredentials?
        do {
            credentials = try await turnService.issueCredentials()
        } catch {
            try Task.checkCancellation()
            guard allowsDirectFallback else { throw error }
            credentials = nil
        }

        let configuration = RTCConfiguration()
        configuration.iceServers = credentials.map { makeIceServers(from: $0) } ?? []
        configuration.sdpSemantics = .unifiedPlan
        configuration.iceCandidatePoolSize = iceCandidatePoolSize
        configuration.iceTransportPolicy = .all
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
