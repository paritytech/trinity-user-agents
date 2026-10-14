import AsyncExtensions
import CommonService
import Foundation
import SubstrateSdk

struct FiatOnrampTransactionStatusPayload: Equatable, Hashable {
    enum Status: Equatable, Hashable {
        case funding
        case inProgress(remainedTime: TimeInterval, amountIn: Balance, amountOut: Balance)
        case completed(amountIn: Balance, amountOut: Balance)
        case failed
    }

    let id: FiatOnRampTransactionId
    let status: Status
}

protocol FiatOnrampTrackingServiceProtocol: AnyObject, ApplicationServiceProtocol {
    func startTracking(sessionId: FiatOnRampSessionId)
    func handleBuySuccess(for sessionId: FiatOnRampSessionId)
    func subscribeToTransactionStatuses() async -> AnyAsyncSequence<Set<FiatOnrampTransactionStatusPayload>>
    func removeFailedTransactions()
    func removeCompletedTransactions()
}

/// Tracks fiat on-ramp transactions and their autoswap lifecycle.
///
/// A typical flow of the transaction is as follows:
/// - A `sessionId` is created and stored in the storage.
/// - The transaction is discovered by calling `fiatOnrampService.fetchTransactions` with the `sessionId`.
/// - If discovered, the initial status of the transaction is emitted, normally `.funding`. The transaction is stored as
///   tracked in storage, and the `sessionId` is removed.
/// - Further, polling of `fiatOnrampService.fetchTransaction` is performed with the `transactionId` to get the
/// transaction status updates.
/// - Polling is performed until there is a terminal state - `funded` or `failed`.
/// - When a transaction is funded, its public status is still `funding` until `DepositService` emits a deposit
///   execution.
/// - When a deposit execution is received, the transaction will be moved to `inProgress` status.
/// - Further the transaction status is updated based on the deposit execution status - `inProgress`, `completed` or
/// `failed`.
/// - The tracked transaction is not removed from storage upon having a final status, be it `.funding(.failed)` or
///   `.swapping(.completed)`/`.swapping(.failed)`; it is removed on user action.
///   It will not participate further in the update pipeline.
///
/// Assumptions:
/// - All transactions that are in `.funding(.funded)`, once a swap is started, will be moved to
/// `.swapping(.inProgress)` status.
///   Assumption is that the respective transactions triggered the swap.
/// - If during an existing swap, a new transaction is funded, it will not be updated with the progress of the swap. A
/// new swap should happen for it to be put in `swapping` state.
public final class FiatOnrampTrackingServicing: FiatOnrampTrackingServiceProtocol {
    enum Timing {
        // Speculative polling interval, will likely have a different value after testing against a real backend
        static let pollingInterval: Duration = .seconds(60)
        static let streamRetryDelay: Duration = .seconds(1)
        static let sessionDiscoveryTtl: TimeInterval = 4 * 60 * 60
    }

    enum TriggerEvent {
        case trackSession(id: FiatOnRampSessionId)
        case pollTransactions
        case autoSwap([DepositExecutionItem])
        case discoverTransactions
        case discoverTransactionsForSession(id: FiatOnRampSessionId)
        case removeFailedTransactions
        case removeCompletedTransactions
    }

    /// Observes auto-swap updates once an account is funded.
    let depositService: DepositServiceProtocol
    /// Service used for polling on-ramp transaction status.
    let fiatOnrampService: FiatOnrampServicing
    /// Storage for tracked fiat on-ramp transactions.
    let fiatOnrampStorage: FiatOnrampStoring
    let clock: any Clock<Duration>
    let dateBuilder: () -> Date
    let logger: LoggerProtocol

    let transactionStatuses: AsyncReplaySubject<Set<FiatOnrampTransactionStatusPayload>> = .init(bufferSize: 1)
    /// Serial trigger stream to avoid concurrent execution across update entry points.
    let updateTriggerEventSequence: AsyncReplaySubject<TriggerEvent> = .init(bufferSize: 3)

    var updateTriggersTask: Task<Void, Never>?
    var sessionDiscoveryTask: Task<Void, Never>?
    var transactionPollingTask: Task<Void, Never>?
    var autoSwapDepositsTask: Task<Void, Never>?
    private var isSetup = false

    init(
        depositService: DepositServiceProtocol,
        fiatOnrampService: FiatOnrampServicing,
        fiatOnrampStorage: FiatOnrampStoring,
        clock: any Clock<Duration>,
        dateBuilder: @escaping () -> Date = Date.init,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.depositService = depositService
        self.fiatOnrampService = fiatOnrampService
        self.fiatOnrampStorage = fiatOnrampStorage
        self.clock = clock
        self.dateBuilder = dateBuilder
        self.logger = logger
    }

    deinit {
        throttle()
    }

    public func setup() {
        guard !isSetup else {
            return
        }

        isSetup = true
        startUpdateTriggersTask()
        startObservationTasks()
    }

    public func throttle() {
        guard isSetup else {
            return
        }

        cancelObservationTasks()
        isSetup = false
    }

    func startTracking(sessionId: FiatOnRampSessionId) {
        updateTriggerEventSequence.send(.trackSession(id: sessionId))
    }

    func handleBuySuccess(for sessionId: FiatOnRampSessionId) {
        updateTriggerEventSequence.send(.discoverTransactionsForSession(id: sessionId))
    }

    func subscribeToTransactionStatuses() async -> AnyAsyncSequence<Set<FiatOnrampTransactionStatusPayload>> {
        transactionStatuses.eraseToAnyAsyncSequence()
    }

    func removeFailedTransactions() {
        updateTriggerEventSequence.send(.removeFailedTransactions)
    }

    func removeCompletedTransactions() {
        updateTriggerEventSequence.send(.removeCompletedTransactions)
    }

    private func startObservationTasks() {
        startSessionTransactionDiscoveryTask()
        startPollTransactionStatusesTask()
        startDepositAutoSwapTask()
    }

    private func cancelObservationTasks() {
        updateTriggersTask?.cancel()
        sessionDiscoveryTask?.cancel()
        transactionPollingTask?.cancel()
        autoSwapDepositsTask?.cancel()

        updateTriggersTask = nil
        sessionDiscoveryTask = nil
        transactionPollingTask = nil
        autoSwapDepositsTask = nil
    }
}
