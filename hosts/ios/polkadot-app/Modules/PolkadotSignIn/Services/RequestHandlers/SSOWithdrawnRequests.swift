import Foundation
import os

/// Requests the pairing host has withdrawn over SSO. Bounded, so a flood of
/// withdrawals for requests that never arrive forgets the oldest first.
final class SSOWithdrawnRequests: Sendable {
    static let defaultCapacity = 64

    private let capacity: Int
    private let messageIds = OSAllocatedUnfairLock<[String]>(initialState: [])

    init(capacity: Int = SSOWithdrawnRequests.defaultCapacity) {
        self.capacity = capacity
    }

    func insert(_ messageId: String) {
        messageIds.withLock { [capacity] ids in
            guard !ids.contains(messageId) else {
                return
            }

            ids.append(messageId)

            if ids.count > capacity {
                ids.removeFirst(ids.count - capacity)
            }
        }
    }

    func contains(_ messageId: String) -> Bool {
        messageIds.withLock { $0.contains(messageId) }
    }
}
