import Foundation
import Individuality
import os
import SubstrateSdk

final class StubNotificationAllocator: NotificationStatementAccountAllocating, @unchecked Sendable {
    var period: UInt32 = 100
    var statementSize = 10 * 1_024
    var hasFreeSlot = true
    var freeSlotCount: Int?
    var awaitResult: Result<Void, Error> = .success(())
    private let initiated = OSAllocatedUnfairLock<[AccountId]>(initialState: [])

    var initiatedTargets: [AccountId] {
        initiated.withLock { $0 }
    }

    func currentPeriod() async throws -> UInt32 {
        period
    }

    func maxStatementSize() async throws -> Int {
        statementSize
    }

    func initiateAllocations(for targets: [AccountId]) async throws -> [AccountId] {
        guard hasFreeSlot else { return [] }

        let scheduled = freeSlotCount.map { Array(targets.prefix($0)) } ?? targets
        initiated.withLock { $0.append(contentsOf: scheduled) }
        return scheduled
    }

    func awaitAllocated(_: AccountId, timeout _: Duration) async throws {
        try awaitResult.get()
    }
}
