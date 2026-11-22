import Foundation
import WebRTC
import StructuredConcurrency

protocol WebRTCConfigMaking: Sendable {
    func makeConnectionConfiguration() async throws -> RTCConfiguration
    func makeDataChannelConfiguration() -> RTCDataChannelConfiguration
}

final class WebRTCConfigFactory: WebRTCConfigMaking {
    private let turnService: TURNCredentialsProviding
    private let iceCandidatePoolSize: Int32
    private let allowsDirectFallback: Bool
    private let clock: any Clock<Duration>

    init(
        turnService: TURNCredentialsProviding,
        iceCandidatePoolSize: Int32 = 8,
        allowsDirectFallback: Bool = false,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.turnService = turnService
        self.iceCandidatePoolSize = iceCandidatePoolSize
        self.allowsDirectFallback = allowsDirectFallback
        self.clock = clock
    }

    func makeConnectionConfiguration() async throws -> RTCConfiguration {
        try Task.checkCancellation()
        let iceServers: [RTCIceServer]
        do {
            let credentials: TURNCredentials
            if allowsDirectFallback {
                credentials = try await issueOptionalCredentials()
            } else {
                credentials = try await turnService.issueCredentials()
            }
            iceServers = makeIceServers(from: credentials)
        } catch {
            try Task.checkCancellation()
            guard allowsDirectFallback else { throw error }
            // Media can connect directly when the optional TURN service is unavailable.
            iceServers = []
        }
        try Task.checkCancellation()

        let configuration = RTCConfiguration()
        configuration.iceServers = iceServers
        configuration.sdpSemantics = .unifiedPlan
        if allowsDirectFallback {
            configuration.bundlePolicy = .maxBundle
            configuration.rtcpMuxPolicy = .require
        }
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
    /// A task group waits for cancelled children, so it cannot bound an uncooperative provider.
    /// The stream races completion against the deadline without waiting for the losing task.
    func issueOptionalCredentials() async throws -> TURNCredentials {
        let (stream, continuation) = AsyncThrowingStream<TURNCredentials, Error>.makeStream()
        let provider = Task { [turnService] in
            do {
                continuation.yield(try await turnService.issueCredentials())
                continuation.finish()
            } catch {
                continuation.finish(throwing: error)
            }
        }
        let deadline = Task { [clock] in
            do {
                // Leave time for peer construction inside the core's five-second deadline.
                try await clock.sleep(for: .seconds(2))
                continuation.finish(throwing: TimeoutError())
            } catch {
                // Completion or outer cancellation has already ended the race.
            }
        }
        defer {
            // A Media deadline leaves the provider's bounded request warming its shared cache.
            if Task.isCancelled { provider.cancel() }
            deadline.cancel()
        }
        var iterator = stream.makeAsyncIterator()
        guard let credentials = try await iterator.next() else {
            try Task.checkCancellation()
            throw TimeoutError()
        }
        return credentials
    }

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
