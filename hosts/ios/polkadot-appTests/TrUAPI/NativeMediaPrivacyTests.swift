import Foundation
import Testing
import WebRTC
@testable import polkadot_app

struct NativeMediaPrivacyTests {
    private struct Credentials: TURNCredentialsProviding {
        let servers: [String]

        func issueCredentials() async throws -> TURNCredentials {
            TURNCredentials(servers: servers, username: "test", password: "test", ttl: 60)
        }
    }

    @Test func hostMayUseDirectAndRelayedConnectivity() async throws {
        let factory = WebRTCConfigFactory(
            turnService: Credentials(servers: ["stun:example.invalid", "turn:example.invalid"]),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceTransportPolicy == .all)
        #expect(configuration.iceServers.flatMap(\.urlStrings) == ["stun:example.invalid", "turn:example.invalid"])
    }

    @Test func unconfiguredServersPermitDirectConnectivity() async throws {
        let factory = WebRTCConfigFactory(
            turnService: Credentials(servers: []),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceTransportPolicy == .all)
        #expect(configuration.iceServers.isEmpty)
    }

    private struct UnavailableCredentials: TURNCredentialsProviding {
        struct Unavailable: Error {}

        func issueCredentials() async throws -> TURNCredentials { throw Unavailable() }
    }

    @Test func unavailableCredentialsDoNotBlockMediaPeers() async throws {
        let factory = WebRTCConfigFactory(
            turnService: UnavailableCredentials(),
            allowsDirectFallback: true
        )
        let configuration = try await factory.makeConnectionConfiguration()
        #expect(configuration.iceTransportPolicy == .all)
        #expect(configuration.iceServers.isEmpty)
    }

    @Test func legacyCallConfigurationRetainsCredentialFailure() async {
        let factory = WebRTCConfigFactory(turnService: UnavailableCredentials())
        await #expect(throws: UnavailableCredentials.Unavailable.self) {
            try await factory.makeConnectionConfiguration()
        }
    }
}
