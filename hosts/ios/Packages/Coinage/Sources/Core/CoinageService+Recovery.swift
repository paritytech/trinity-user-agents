import AsyncAlgorithms
import AsyncExtensions
import Foundation
import TrUAPIHost

/// Recovery as the wallet screens show it. The core reconstructs the wallet on its own when that is
/// due; what is shown here is the scan of the wallet the app handed over, and the deep search the
/// user asks for.
extension CoinageService {
    public func subscribeBackupProgress() -> AnyAsyncSequence<BackupProgress> {
        let core = core
        let deepSearch = deepSearch
        let acknowledged = acknowledged

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    for try await (handOver, search, isAcknowledged) in combineLatest(
                        core.handOverProgress(),
                        deepSearch,
                        acknowledged
                    ) {
                        continuation.yield(
                            Self.backupProgress(handOver: handOver, search: search, isAcknowledged: isAcknowledged)
                        )
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .removeDuplicates()
        .eraseToAnyAsyncSequence()
    }

    public func deepSearchBackup() async {
        guard deepSearch.value?.isInProgress != true else { return }

        deepSearch.send(.deep(.syncing))
        setAcknowledged(false)

        do {
            let id = try CoinageOperationId.random()
            let search = followOperation(id) { coinage in
                try await coinage.reconstruct(id: id.value, deep: true)
            }
            for try await _ in search {}
            deepSearch.send(.deep(.completed))
        } catch {
            logger?.error("Deep search failed: \(error)")
            deepSearch.send(.deep(.failed))
        }
    }

    public func markBackupAsCompleted() async {
        guard deepSearch.value?.isInProgress != true else { return }

        setAcknowledged(true)
        deepSearch.send(nil)
    }

    public func subscribeAccountBackupStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus> {
        let registryConfigured = registryConfigured

        return recoverability()
            .map { recoverability -> CoinageAccountBackupStatus in
                if case .registered = recoverability {
                    return .completed
                }
                return registryConfigured ? .registering : .delayed
            }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }

    public func subscribeAwaitingInstallationBackup() -> AnyAsyncSequence<Bool> {
        let registryConfigured = registryConfigured

        return recoverability()
            .map { recoverability -> Bool in
                guard registryConfigured, case .pending = recoverability else { return false }
                return true
            }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}

private extension CoinageService {
    static func backupProgress(
        handOver: HandOverProgress,
        search: BackupProgress?,
        isAcknowledged: Bool
    ) -> BackupProgress {
        if let search, search.isInProgress || !isAcknowledged {
            return search
        }
        if handOver == .scanning {
            return .initial(.syncing)
        }
        if isAcknowledged {
            return .completed
        }
        return handOver == .done ? .initial(.completed) : .completed
    }

    func setAcknowledged(_ value: Bool) {
        recoveryAcknowledgement.save(isAcknowledged: value)
        acknowledged.send(value)
    }

    /// The current installation's recoverability, read again whenever the core reports it may have changed.
    func recoverability() -> AsyncThrowingStream<Recoverability, Error> {
        let core = core

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    continuation.yield(try await core.use { try await $0.recoverable() })
                    for try await event in core.events() {
                        switch event {
                        case let .recoverability(recoverability):
                            continuation.yield(recoverability)
                        case .resynced:
                            continuation.yield(try await core.use { try await $0.recoverable() })
                        default:
                            continue
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
