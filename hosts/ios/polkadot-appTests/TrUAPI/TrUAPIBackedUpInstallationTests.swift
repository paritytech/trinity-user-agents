import AsyncExtensions
import Coinage
import Foundation
import SubstrateSdk
import Testing
@testable import polkadot_app

struct TrUAPIBackedUpInstallationTests {
    /// The runtime is built before Coinage, so a product can ask for an installation first. It must
    /// get the backed-up one once Coinage arrives rather than a failure or a key under no backup.
    @Test
    func aRequestBeforeCoinageAttachesWaitsForTheBackedUpInstallation() async throws {
        let installation = try CoinageInstallationId(value: Data.randomOrError(of: CoinageInstallationId.sizeBytes))
        let backedUpInstallation = TrUAPIBackedUpInstallation()

        let pending = Task { try await backedUpInstallation.installationId() }
        backedUpInstallation.attach(FixedBackedUpInstallation(installation: installation))

        #expect(try await pending.value == installation.value)
    }
}

private struct FixedBackedUpInstallation: CoinageBackedUpInstallationProviding {
    let installation: CoinageInstallationId

    func backedUpInstallation() async throws -> CoinageInstallationId {
        installation
    }

    func subscribeAwaitingBackup() -> AnyAsyncSequence<Bool> {
        AsyncStream<Bool> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}
