import AsyncExtensions
import BackgroundExecution
import DurableTransactions
import Foundation
import os
import SDKLogger

/// Whether this installation is recorded on chain, so that a reinstall can find the coins it allocates.
/// Until it is, balance held in this installation's subtree could not be recovered from the seed alone.
public enum CoinageAccountBackupStatus: Hashable, Sendable {
    case registering
    /// Not final within the expected time, or a reorg dropped it after it was included.
    case delayed
    case completed

    public var needsAttention: Bool { self == .delayed }
}

public protocol CoinageAccountBackupObserving: Sendable {
    /// The on-chain registration of this installation, for surfacing a backup that has not yet landed.
    func subscribeStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus>
}

protocol CoinageInstallationRegistering: CoinageAccountBackupObserving {
    /// Starts the one registration run of this process; later calls are no-ops.
    func start()
}

/// Registers this installation in the `AccountDataStore` contract, once per process, and reports how
/// it goes.
///
/// The durable engine decides what each attempt did; the loop here only decides when another attempt
/// is needed. It never submits while one is live: the oracle reads the same record for every attempt,
/// so two in flight would both be credited with it.
final class CoinageInstallationRegistrar: CoinageInstallationRegistering, @unchecked Sendable {
    struct Timing: Sendable {
        /// Overdue counts from the start, so a contract address that never arrives is reported like any
        /// other delay.
        var expectedRegistrationTime: Duration = .seconds(30)
        var initialBackoff: Duration = .seconds(5)
        var maxBackoff: Duration = .seconds(5 * 60)
        var maxBackoffDoublings = 10
        var clock: any Clock<Duration> = ContinuousClock()
    }

    private let currentInstallationStore: any CoinageCurrentInstallationStoring
    private let configProvider: any AccountDataStoreConfigProviding
    private let engine: any DurableTxServicing
    private let submitter: any InstallationRegistrationSubmitting
    private let backgroundExecutor: any BackgroundExecuting
    private let timing: Timing
    private let logger: (any SDKLoggerProtocol)?

    private let status = AsyncCurrentValueSubject<CoinageAccountBackupStatus>(.registering)
    private let run = OSAllocatedUnfairLock<Task<Void, Never>?>(initialState: nil)
    private let completed = OSAllocatedUnfairLock(initialState: false)

    init(
        currentInstallationStore: any CoinageCurrentInstallationStoring,
        configProvider: any AccountDataStoreConfigProviding,
        engine: any DurableTxServicing,
        submitter: any InstallationRegistrationSubmitting,
        backgroundExecutor: any BackgroundExecuting,
        timing: Timing = Timing(),
        logger: (any SDKLoggerProtocol)?
    ) {
        self.currentInstallationStore = currentInstallationStore
        self.configProvider = configProvider
        self.engine = engine
        self.submitter = submitter
        self.backgroundExecutor = backgroundExecutor
        self.timing = timing
        self.logger = logger
    }

    deinit {
        run.withLock { $0?.cancel() }
    }

    func subscribeStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus> {
        status.eraseToAnyAsyncSequence()
    }

    func start() {
        run.withLock { task in
            guard task == nil else { return }
            task = Task { [weak self] in await self?.register() }
        }
    }
}

private extension CoinageInstallationRegistrar {
    func register() async {
        let overdue = Task { [timing, weak self] in
            do {
                try await timing.clock.sleep(for: timing.expectedRegistrationTime)
            } catch {
                return // cancelled: the run ended, there is nothing to be late for
            }
            self?.reportDelayedUnlessCompleted()
        }
        defer { overdue.cancel() }

        guard let target = await awaitTarget() else { return }
        await registerUntilFinalized(target)
        reportCompleted()
    }

    /// Under the lock, so the overdue timer cannot overwrite a `completed` that lands between its check
    /// and its send — that would leave the warning up for a registration that has already finalized.
    func reportDelayedUnlessCompleted() {
        completed.withLock { completed in
            guard !completed else { return }
            send(.delayed)
        }
    }

    func reportCompleted() {
        completed.withLock { completed in
            completed = true
            send(.completed)
        }
    }

    func send(_ newStatus: CoinageAccountBackupStatus) {
        guard status.value != newStatus else { return }
        logger?.info("Installation registration status=\(newStatus)")
        status.send(newStatus)
    }

    /// `nil` only when the run is cancelled before a contract address arrives.
    func awaitTarget() async -> InstallationRegistrationTarget? {
        while !Task.isCancelled {
            do {
                let installation = try await currentInstallationStore.getOrCreateCurrent()
                if let contract = await configProvider.contractAddress() {
                    return InstallationRegistrationTarget(contract: contract, installation: installation)
                }
                logger?.warning("Installation registration: data store contract address is not available yet")
            } catch {
                logger?.error("Installation registration: could not read the current installation: \(error)")
            }
            try? await timing.clock.sleep(for: timing.initialBackoff)
        }
        return nil
    }

    func registerUntilFinalized(_ target: InstallationRegistrationTarget) async {
        logger?.info("Installation registration starting, \(target.logDescription)")
        engine.startRecoveryPass()

        while !Task.isCancelled {
            do {
                try await attemptUntilFinalized(target)
                return
            } catch {
                logger?.error("Installation registration run interrupted: \(error)")
                try? await timing.clock.sleep(for: timing.initialBackoff)
            }
        }
    }

    func attemptUntilFinalized(_ target: InstallationRegistrationTarget) async throws {
        var attempts = 0

        while !Task.isCancelled {
            let settled = try await awaitNoLiveAttempt(target)

            if settled.contains(where: { $0.status == .finalizedSuccess }) {
                logger?.info("Installation registration finalized after \(attempts) attempt(s)")
                return
            }

            // Only attempts of this run count: failures an earlier launch left behind say nothing about now.
            if attempts > 0 {
                let backoff = backoff(after: attempts)
                logger?.warning("Installation registration attempt \(attempts) did not land, retrying in \(backoff)")
                try await timing.clock.sleep(for: backoff)
            }

            attempts += 1
            logger?.info("Installation registration attempt \(attempts)")
            do {
                // One background assertion per attempt. This loop can run for the whole process, and an
                // assertion around it would retain every connection that long; when iOS expired it the
                // loop would end and the backoff start over.
                _ = try await backgroundExecutor.execute { [submitter] in
                    try await submitter.submitAttempt(target: target)
                }
            } catch {
                logger?.error("Installation registration attempt \(attempts) could not be submitted: \(error)")
            }
        }
    }

    /// Subscribed afresh after every attempt, so the first emission is read after that attempt committed.
    func awaitNoLiveAttempt(_ target: InstallationRegistrationTarget) async throws -> [DurableTxEntry] {
        let entries = engine.subscribeGroupEntries(domain: .coinageInstallation, groupId: target.registrationGroup)
        for try await snapshot in entries where snapshot.allSatisfy({ !$0.status.isLive }) {
            return snapshot
        }
        throw CancellationError()
    }

    func backoff(after attempts: Int) -> Duration {
        let doublings = min(attempts - 1, timing.maxBackoffDoublings)
        let exponential = timing.initialBackoff * (1 << doublings)
        return min(exponential, timing.maxBackoff)
    }
}
