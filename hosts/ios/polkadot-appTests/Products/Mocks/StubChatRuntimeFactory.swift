import Foundation
import os
import Products
@testable import polkadot_app

/// Hands out `runtimes` in order and throws once they run out.
final class StubChatRuntimeFactory: ChatRuntimeMaking, @unchecked Sendable {
    struct Exhausted: Error {}

    private let remaining: OSAllocatedUnfairLock<[ChatRuntimeProtocol]>

    init(runtimes: [ChatRuntimeProtocol]) {
        remaining = OSAllocatedUnfairLock(uncheckedState: runtimes)
    }

    func makeRuntime(for _: ResolvedProduct) throws -> ChatRuntimeProtocol {
        guard let runtime = remaining.withLock({ $0.isEmpty ? nil : $0.removeFirst() }) else {
            throw Exhausted()
        }

        return runtime
    }
}
