import Foundation
import os

@testable import polkadot_app

final class AllowanceSlotsRenewalSpy: AllowanceSlotsRenewing, @unchecked Sendable {
    private let calls = OSAllocatedUnfairLock(initialState: 0)

    var callCount: Int {
        calls.withLock { $0 }
    }

    func renewAllowanceSlots() async {
        calls.withLock { $0 += 1 }
    }
}
