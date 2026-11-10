import Foundation
import Testing
import TrUAPIHost

struct PublicProfileStreamTests {
    @Test(.timeLimit(.minutes(1)))
    func anInvalidUsernameTerminatesWithTheCoreDomainError() async throws {
        let runtime = try TrUAPIHostRuntime(bridge: StubHostBridge(), runtimeConfig: HostRuntimeConfig(
            hostName: "profile-tests",
            peopleChainGenesisHash: Data(repeating: 0, count: 32),
            bulletinChainGenesisHash: Data(repeating: 0, count: 32),
            assetHubChainGenesisHash: Data(repeating: 1, count: 32),
            networkSuffix: "paseo",
            databaseDirectory: temporaryDatabaseDirectory()
        ))
        defer { withExtendedLifetime(runtime) {} }
        var received: [PublicProfile] = []

        do {
            for try await profile in runtime.resolvePublicProfile(
                username: "invalid username",
                options: .init(forceFresh: false, skipStale: false)
            ) {
                received.append(profile)
            }
            Issue.record("An invalid username must terminate the profile stream with an error")
        } catch let error as PublicProfileError {
            guard case .InvalidUsername = error else {
                Issue.record("Expected an invalid username error, received \(error)")
                return
            }
        }

        #expect(received == [])
    }
}
