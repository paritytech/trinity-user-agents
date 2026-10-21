import Foundation
import os
import ChainRegistry

final class RecordingChainSocketDelegate: ChainSocketDelegate, @unchecked Sendable {
    private struct Events {
        var received: [String] = []
        var closeReasons: [ChainSocketCloseReason] = []
    }

    private let events = OSAllocatedUnfairLock(uncheckedState: Events())

    var onClose: ((ChainSocketCloseReason) -> Void)?

    var received: [String] { events.withLockUnchecked { $0.received } }
    var closeReasons: [ChainSocketCloseReason] { events.withLockUnchecked { $0.closeReasons } }

    func chainSocket(_: ChainSocketProtocol, didReceive text: String) {
        events.withLockUnchecked { $0.received.append(text) }
    }

    func chainSocket(_: ChainSocketProtocol, didCloseWith reason: ChainSocketCloseReason) {
        events.withLockUnchecked { $0.closeReasons.append(reason) }
        onClose?(reason)
    }
}
