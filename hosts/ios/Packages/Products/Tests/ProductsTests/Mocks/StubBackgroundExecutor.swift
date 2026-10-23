import Foundation
import os
@testable import Products

final class StubBackgroundExecutor: ProductLoadBackgroundExecuting, @unchecked Sendable {
    private let expiringRuns: OSAllocatedUnfairLock<Int>

    init(expiringRuns: Int = 0) {
        self.expiringRuns = OSAllocatedUnfairLock(initialState: expiringRuns)
    }

    func execute<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) async throws -> T {
        let expires = expiringRuns.withLock { runs in
            guard runs > 0 else { return false }
            runs -= 1
            return true
        }

        guard !expires else {
            throw ProductLoadBackgroundExpiredError()
        }

        return try await operation()
    }
}
