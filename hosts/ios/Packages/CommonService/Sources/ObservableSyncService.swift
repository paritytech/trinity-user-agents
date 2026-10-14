import Foundation

public protocol ObservableSyncServiceProtocol: SyncServiceProtocol {
    func subscribeSyncState(
        _ target: AnyObject,
        queue: DispatchQueue?,
        closure: @escaping (Bool, Bool) -> Void
    )

    func unsubscribeSyncState(_ target: AnyObject)

    func hasSubscription(for target: AnyObject) -> Bool
}

open class ObservableSyncService: BaseSyncService, ObservableSyncServiceProtocol {
    private let syncState = Observable<Bool>(state: false)

    override public var isSyncing: Bool {
        didSet {
            updateSyncState()
        }
    }

    override public var retryAttempt: Int {
        didSet {
            updateSyncState()
        }
    }

    public func subscribeSyncState(
        _ target: AnyObject,
        queue: DispatchQueue?,
        closure: @escaping (Bool, Bool) -> Void
    ) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        syncState.addObserver(with: target, queue: queue, closure: closure)
    }

    public func unsubscribeSyncState(_ target: AnyObject) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        syncState.removeObserver(by: target)
    }

    public func hasSubscription(for target: AnyObject) -> Bool {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return syncState.hasObserver(target)
    }

    private func updateSyncState() {
        syncState.state = isSyncing || retryAttempt > 0
    }
}
