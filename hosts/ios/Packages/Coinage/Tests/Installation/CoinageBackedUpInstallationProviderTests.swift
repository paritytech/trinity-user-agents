import Foundation
import Testing
@testable import Coinage

struct CoinageBackedUpInstallationProviderTests {
    private let store = StubCurrentInstallationStore(current: .test)
    private let backup = StubAccountBackupObserver()
    private let provider: CoinageBackedUpInstallationProvider

    init() {
        provider = CoinageBackedUpInstallationProvider(installationStore: store, backupObserver: backup)
    }

    @Test("a completed backup hands out the installation at once, without reporting a wait")
    func completedBackup() async throws {
        backup.status.send(.completed)

        let installation = try await provider.backedUpInstallation()

        #expect(installation == .test)
        #expect(try await firstAwaiting() == false)
    }

    @Test(arguments: [CoinageAccountBackupStatus.registering, .delayed])
    func pendingBackupWaitsUntilCompleted(status: CoinageAccountBackupStatus) async throws {
        backup.status.send(status)
        let request = Task { try await provider.backedUpInstallation() }

        try await waitUntilAwaiting(true)
        backup.status.send(.completed)

        #expect(try await request.value == .test)
        try await waitUntilAwaiting(false)
    }

    @Test("the wait is reported once for any number of concurrent callers")
    func concurrentCallers() async throws {
        let requests = (0 ..< 3).map { _ in Task { try await provider.backedUpInstallation() } }
        try await waitUntilAwaiting(true)

        backup.status.send(.completed)

        for request in requests {
            #expect(try await request.value == .test)
        }
        try await waitUntilAwaiting(false)
    }

    @Test("a cancelled caller stops counting as waiting")
    func cancelledCaller() async throws {
        let request = Task { try await provider.backedUpInstallation() }
        try await waitUntilAwaiting(true)

        request.cancel()

        try await waitUntilAwaiting(false)
        await #expect(throws: CancellationError.self) { try await request.value }
    }
}

private extension CoinageBackedUpInstallationProviderTests {
    func firstAwaiting() async throws -> Bool? {
        for try await awaiting in provider.subscribeAwaitingBackup() {
            return awaiting
        }
        return nil
    }

    func waitUntilAwaiting(_ expected: Bool) async throws {
        for try await awaiting in provider.subscribeAwaitingBackup() where awaiting == expected {
            return
        }
    }
}
