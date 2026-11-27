@testable import polkadot_app
import os

final class StubPendingOutgoingChatRequestLoader: PendingOutgoingChatRequestLoading, @unchecked Sendable {
    private let pending: OSAllocatedUnfairLock<OutgoingChatRequestInput?>

    init(pending: OutgoingChatRequestInput?) {
        self.pending = OSAllocatedUnfairLock(initialState: pending)
    }

    func setPending(_ pending: OutgoingChatRequestInput?) {
        self.pending.withLock { $0 = pending }
    }

    func pendingRequest(requestId _: String) async throws -> OutgoingChatRequestInput? {
        pending.withLock { $0 }
    }
}
