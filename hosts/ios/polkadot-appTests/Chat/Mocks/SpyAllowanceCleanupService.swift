import Foundation
import os
@testable import polkadot_app

final class SpyAllowanceCleanupService: AllowanceCleanupServicing, @unchecked Sendable {
    private let calls = OSAllocatedUnfairLock(initialState: 0)
    var onCleanup: (() async -> Void)?

    var cleanupCount: Int { calls.withLock { $0 } }

    func cleanup() async {
        calls.withLock { $0 += 1 }
        await onCleanup?()
    }
}
