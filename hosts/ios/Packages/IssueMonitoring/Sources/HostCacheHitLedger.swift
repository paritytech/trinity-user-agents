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

    private let claimed = OSAllocatedUnfairLock(initialState: Set<Key>())

    public init() {}

    func claim(step: String, productID: String, context: HostLoadContext) -> Bool {
        let key = Key(
            step: step,
            productID: productID,
            sessionID: context.sessionID,
            trigger: context.sessionID == nil ? context.trigger : .open
        )

        return claimed.withLock { $0.insert(key).inserted }
    }
}
