import Foundation
import StructuredConcurrency
import os

@testable import polkadot_app

final class SharedRunSpy<Value: Sendable>: SharedRunning, @unchecked Sendable {
    private let calls = OSAllocatedUnfairLock(initialState: 0)

    var callCount: Int {
        calls.withLock { $0 }
    }

    func run(_ operation: @escaping @Sendable () async -> Value) async -> Value {
        calls.withLock { $0 += 1 }
        return await operation()
    }
}
