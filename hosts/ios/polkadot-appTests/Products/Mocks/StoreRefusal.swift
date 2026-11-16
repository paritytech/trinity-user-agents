import Foundation

/// What a `StoreRefusingStorageFacade` refuses, shared by the mapper that reads and the service that writes.
final class StoreRefusal: @unchecked Sendable {
    private let lock = NSLock()
    private var readsBeforeRefusal: Int?
    private var writes = false

    var refusesWrites: Bool {
        get { lock.withLock { writes } }
        set { lock.withLock { writes = newValue } }
    }

    func refuseReads(after allowedReads: Int) {
        lock.withLock { readsBeforeRefusal = allowedReads }
    }

    func allowsRead() -> Bool {
        lock.withLock {
            guard let remaining = readsBeforeRefusal else {
                return true
            }
            readsBeforeRefusal = max(remaining - 1, 0)
            return remaining > 0
        }
    }
}
