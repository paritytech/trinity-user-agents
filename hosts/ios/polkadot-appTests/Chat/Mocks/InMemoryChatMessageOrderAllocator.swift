import Foundation
import os

@testable import polkadot_app

final class InMemoryChatMessageOrderAllocator: ChatMessageOrderAllocating {
    private let lock = OSAllocatedUnfairLock()
    private var counter: UInt64?

    func nextOrder(floor: () throws -> UInt64) throws -> UInt64 {
        lock.lock()
        defer { lock.unlock() }

        let nextValue = try (counter ?? floor()) + 1
        counter = nextValue
        return nextValue
    }
}
