import AsyncExtensions
import Foundation
import os

/// The one way allocators learn the current installation: only once its on-chain registration has
/// completed, so nothing is allocated into a subtree a reinstall could not find from the seed.
public protocol CoinageBackedUpInstallationProviding: Sendable {
    /// The current installation, returned once its backup reaches ``CoinageAccountBackupStatus/completed``.
    func backedUpInstallation() async throws -> CoinageInstallationId

    /// Whether a caller is waiting for the backup right now; replays the current value.
    func subscribeAwaitingBackup() -> AnyAsyncSequence<Bool>
}

final class CoinageBackedUpInstallationProvider: CoinageBackedUpInstallationProviding, @unchecked Sendable {
    private let installationStore: any CoinageCurrentInstallationStoring
    private let backupObserver: any CoinageAccountBackupObserving
    private let awaiting = AsyncCurrentValueSubject<Bool>(false)
    private let waiters = OSAllocatedUnfairLock(initialState: 0)

    init(
        installationStore: any CoinageCurrentInstallationStoring,
        backupObserver: any CoinageAccountBackupObserving
    ) {
        self.installationStore = installationStore
        self.backupObserver = backupObserver
    }

    func backedUpInstallation() async throws -> CoinageInstallationId {
        try await awaitBackupCompleted()
        return try await installationStore.getOrCreateCurrent()
    }

    func subscribeAwaitingBackup() -> AnyAsyncSequence<Bool> {
        awaiting.eraseToAnyAsyncSequence()
    }
}

private extension CoinageBackedUpInstallationProvider {
    func awaitBackupCompleted() async throws {
        var waiting = false
        defer { if waiting { removeWaiter() } }

        for try await status in backupObserver.subscribeStatus() {
            if status == .completed { return }
            if !waiting {
                waiting = true
                addWaiter()
            }
        }
        throw CancellationError()
    }

    func addWaiter() {
        waiters.withLock { count in
            count += 1
            if count == 1 { awaiting.send(true) }
        }
    }

    func removeWaiter() {
        waiters.withLock { count in
            count -= 1
            if count == 0 { awaiting.send(false) }
        }
    }
}
