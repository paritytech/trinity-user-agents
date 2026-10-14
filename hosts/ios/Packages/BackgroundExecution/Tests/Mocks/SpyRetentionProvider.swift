import Foundation
import ChainRegistry

/// Counts retain calls — total and `.all`-scoped — and returns inert no-op tokens.
/// It retains nothing; it only records how `retainConnections(_:)` was invoked.
final class SpyRetentionProvider: ConnectionRetentionProviding, @unchecked Sendable {
    private let lock = NSLock()
    private var totalRetains = 0
    private var allScopeRetains = 0

    var totalRetainCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return totalRetains
    }

    var allScopeRetainCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return allScopeRetains
    }

    func retainConnections(_ scope: ConnectionRetainScope) -> ConnectionRetainToken {
        lock.lock()
        defer { lock.unlock() }

        totalRetains += 1

        if case .all = scope {
            allScopeRetains += 1
        }

        return ConnectionRetainToken()
    }
}
