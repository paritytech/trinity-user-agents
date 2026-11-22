@testable import polkadot_app
import Foundation
import Testing
import WebRTC

@Suite("WebRTC optional TURN configuration")
struct WebRTCConfigFactoryTests {
    private enum Failure: Error { case unavailable }

    private struct CredentialsProvider: TURNCredentialsProviding {
        let result: Result<TURNCredentials, Error>

        func issueCredentials() async throws -> TURNCredentials {
            try result.get()
        }
    }

    @Test("Media retains direct ICE when TURN acquisition fails")
    func directFallback() async throws {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(Failure.unavailable)),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.isEmpty)
        #expect(configuration.iceTransportPolicy == .all)
        #expect(configuration.sdpSemantics == .unifiedPlan)
        #expect(configuration.iceCandidatePoolSize == 8)
    }

    @Test("Existing Chat callers retain TURN failure behavior")
    func defaultStillThrows() async {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(Failure.unavailable))
        )
        await #expect(throws: Failure.self) {
            try await factory.makeConnectionConfiguration()
        }
    }

    @Test("Media preserves cancellation errors")
    func cancellationIsNotFallback() async {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(CancellationError())),
            allowsDirectFallback: true
        )
        await #expect(throws: CancellationError.self) {
            try await factory.makeConnectionConfiguration()
        }
    }

    @Test("Media preserves cancelled network requests")
    func cancelledRequestIsNotFallback() async {
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .failure(URLError(.cancelled))),
            allowsDirectFallback: true
        )
        await #expect(throws: URLError.self) {
            try await factory.makeConnectionConfiguration()
        }
    }

    @Test("TURN servers supplement rather than replace direct ICE", arguments: [[], ["turn:relay.example:3478"]])
    func availableCredentials(servers: [String]) async throws {
        let credentials = TURNCredentials(servers: servers, username: "test", password: "test", ttl: 60)
        let factory = WebRTCConfigFactory(
            turnService: CredentialsProvider(result: .success(credentials)),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceServers.flatMap(\.urlStrings) == servers)
        #expect(configuration.iceTransportPolicy == .all)
    }
}
