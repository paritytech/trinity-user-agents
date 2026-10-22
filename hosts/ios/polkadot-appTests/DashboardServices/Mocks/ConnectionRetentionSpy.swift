import ChainRegistry
import Foundation
import os

@testable import polkadot_app

final class ConnectionRetentionSpy: BackgroundConnectionRetaining, @unchecked Sendable {
    private struct State {
        var retainCount = 0
        var heldCount = 0
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var retainCount: Int { state.withLock(\.retainCount) }
    var heldCount: Int { state.withLock(\.heldCount) }

    func retainAllConnections() -> any ConnectionTokenProtocol {
        state.withLock { state in
            state.retainCount += 1
            state.heldCount += 1
        }

        return ConnectionTokenSpy { [state] in state.withLock { $0.heldCount -= 1 } }
    }
}

private final class ConnectionTokenSpy: ConnectionTokenProtocol, @unchecked Sendable {
    private let onRelease: () -> Void
    private let isReleased = OSAllocatedUnfairLock(initialState: false)

    init(onRelease: @escaping () -> Void) {
        self.onRelease = onRelease
    }

    func release() {
        let wasReleased = isReleased.withLock { released in
            defer { released = true }
            return released
        }

        if !wasReleased {
            onRelease()
        }
    }
}
