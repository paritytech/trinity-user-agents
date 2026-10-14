import Foundation
import SubstrateSdk
import SubstrateStorageSubscription
import Operation_iOS
import SDKLogger

public protocol ObservableSubscriptionStateProtocol {
    associatedtype TChange: BatchStorageSubscriptionResult

    init(change: TChange) throws

    func merging(change: TChange) -> Self
}

public protocol ObservableSubscriptionSyncServiceProtocol {
    associatedtype TState

    var workQueue: DispatchQueue { get }

    func getState() -> TState?

    func createFetchOperation() -> BaseOperation<TState>
}

public enum ObservableSubscriptionSyncServiceError: Error {
    case unexpectedState
}

open class ObservableSubscriptionSyncService<T: ObservableSubscriptionStateProtocol>: ObservableSyncService,
    ObservableSubscriptionSyncServiceProtocol {
    public typealias TState = T

    let runtimeProvider: RuntimeCodingServiceProtocol
    let connection: JSONRPCEngine
    let repository: AnyDataProviderRepository<ChainStorageItem>?
    let operationQueue: OperationQueue
    public let workQueue: DispatchQueue

    var state: T?
    var subscription: CallbackBatchStorageSubscription<T.TChange>?

    public init(
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        operationQueue: OperationQueue,
        repository: AnyDataProviderRepository<ChainStorageItem>? = nil,
        workQueue: DispatchQueue = .global(),
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(),
        logger: SDKLoggerProtocol
    ) {
        self.connection = connection
        self.runtimeProvider = runtimeProvider
        self.operationQueue = operationQueue
        self.repository = repository
        self.workQueue = workQueue

        super.init(retryStrategy: retryStrategy, logger: logger)
    }

    deinit {
        clearSubscription()
    }

    public func getState() -> T? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return state
    }

    open func getRequests() throws -> [BatchStorageSubscriptionRequest] {
        fatalError("Must be overriden by subsclass")
    }

    override public func stopSyncUp() {
        clearSubscription()
    }

    override public func performSyncUp() {
        do {
            clearSubscription()

            try subscribe()
        } catch {
            completeImmediate(error)
        }
    }

    // MARK: Private

    private func subscribe() throws {
        let requests = try getRequests()

        subscription = CallbackBatchStorageSubscription(
            requests: requests,
            connection: connection,
            runtimeService: runtimeProvider,
            repository: repository,
            operationQueue: operationQueue,
            callbackQueue: workQueue
        ) { [weak self] result in
            self?.mutex.lock()

            self?.handleStateChangeResult(result)

            self?.mutex.unlock()
        }

        subscription?.subscribe()
    }

    private func handleStateChangeResult(_ result: Result<T.TChange, Error>) {
        do {
            let change = try result.get()

            // switch sync state manually if needed to allow others track when new state applied
            if !isSyncing {
                isSyncing = true
            }

            logger.debug("Change: \(change)")

            if let currentState = state {
                state = currentState.merging(change: change)
            } else {
                state = try .init(change: change)
            }

            completeImmediate(nil)
        } catch {
            logger.error("Unexpected error: \(error)")
            completeImmediate(error)
        }
    }

    private func clearSubscription() {
        subscription?.unsubscribe()
        subscription = nil
    }
}

public extension ObservableSubscriptionSyncServiceProtocol where Self: ObservableSyncService {
    func createFetchOperation() -> BaseOperation<TState> {
        let subscriber = NSObject()

        return AsyncClosureOperation(
            operationClosure: { closure in
                if let state = self.getState() {
                    closure(.success(state))

                    return
                }

                self.subscribeSyncState(
                    subscriber,
                    queue: self.workQueue
                ) { _, newIsSyncing in
                    if !newIsSyncing, let state = self.getState() {
                        self.unsubscribeSyncState(subscriber)

                        closure(.success(state))
                    }
                }
            },
            cancelationClosure: {
                self.unsubscribeSyncState(subscriber)
            }
        )
    }
}
