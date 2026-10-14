import Foundation
import Operation_iOS
import SubstrateSdk
import SDKLogger

public protocol SyncServiceProtocol {
    func getIsSyncing() -> Bool
    func getIsActive() -> Bool

    func syncUp(afterDelay: TimeInterval, ignoreIfSyncing: Bool)
    func stopSyncUp()
    func setup()
}

public extension SyncServiceProtocol {
    func syncUp() {
        syncUp(afterDelay: 0, ignoreIfSyncing: true)
    }
}

open class BaseSyncService {
    public let retryStrategy: ReconnectionStrategyProtocol
    public let logger: SDKLoggerProtocol

    public var retryAttempt: Int = 0

    public var isSyncing: Bool = false
    public var isActive: Bool = false

    public let mutex = NSLock()

    private lazy var scheduler: Scheduler = {
        let scheduler = Scheduler(with: self, callbackQueue: DispatchQueue.global())
        return scheduler
    }()

    public init(
        retryStrategy: ReconnectionStrategyProtocol = ExponentialReconnection(),
        logger: SDKLoggerProtocol
    ) {
        self.retryStrategy = retryStrategy
        self.logger = logger
    }

    open func performSyncUp() {
        fatalError("Method must be overriden by child class")
    }

    open func stopSyncUp() {
        fatalError("Method must be overriden by child class")
    }

    public func deactivate() {}

    public func markSyncingImmediate() {
        isSyncing = true
    }

    public func complete(_ error: Error?) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        completeImmediate(error)
    }

    public func completeImmediate(_ error: Error?) {
        guard isActive else {
            return
        }

        isSyncing = false

        if let error {
            logger.error("Sync failed with error: \(error)")

            retryAttempt += 1

            retry()
        } else {
            logger.debug("Sync completed")

            retryAttempt = 0
        }
    }

    public func retry() {
        if let nextDelay = retryStrategy.reconnectAfter(attempt: retryAttempt) {
            logger.debug("Scheduling chain sync retry after \(nextDelay)")

            scheduler.notifyAfter(nextDelay)
        }
    }
}

extension BaseSyncService: ApplicationServiceProtocol {
    public func setup() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard !isActive else {
            return
        }

        isActive = true
        isSyncing = true

        performSyncUp()
    }

    public func throttle() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard isActive else {
            return
        }

        isActive = false

        scheduler.cancel()

        if isSyncing {
            stopSyncUp()
        }

        isSyncing = false
        retryAttempt = 0

        deactivate()
    }
}

extension BaseSyncService: SchedulerDelegate {
    public func didTrigger(scheduler _: SchedulerProtocol) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        isSyncing = true

        performSyncUp()
    }
}

extension BaseSyncService: SyncServiceProtocol {
    public func getIsSyncing() -> Bool {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return isSyncing
    }

    public func getIsActive() -> Bool {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return isActive
    }

    public func syncUp(afterDelay: TimeInterval, ignoreIfSyncing: Bool) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard isActive else {
            return
        }

        if ignoreIfSyncing, isSyncing {
            return
        }

        if isSyncing {
            stopSyncUp()

            isSyncing = false
        }

        if afterDelay > 0 {
            guard !scheduler.isScheduled else {
                return
            }

            scheduler.notifyAfter(afterDelay)

        } else {
            scheduler.cancel()

            isSyncing = true

            performSyncUp()
        }
    }
}
