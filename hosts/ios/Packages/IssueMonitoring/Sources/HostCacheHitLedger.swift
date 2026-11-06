import Foundation
import os

public final class HostCacheHitLedger: Sendable {
    public static let shared = HostCacheHitLedger()

    private struct Key: Hashable {
        let step: String
        let productID: String
        let sessionID: UUID?
        let trigger: HostLoadTrigger
    }

    private static let capacity = 4_096
    private let claimed = OSAllocatedUnfairLock(initialState: Set<Key>())

    public init() {}

    func claim(step: String, productID: String, context: HostLoadContext) -> Bool {
        let key = Key(
            step: step,
            productID: productID,
            sessionID: context.sessionID,
            trigger: context.sessionID == nil ? context.trigger : .open
        )

        return claimed.withLock { claimed in
            // Bounded: a long session opens many products, and an old key only risks one repeated report.
            if claimed.count >= Self.capacity { claimed.removeAll() }
            return claimed.insert(key).inserted
        }
    }
}
