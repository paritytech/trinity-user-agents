import Foundation
import SubstrateSdk

public protocol AllowanceSlotAllocating {
    /// Returns the period the slot was allocated into
    @discardableResult
    func assignSlot(accountId: AccountId, priority: AllowanceRecord.Priority) async throws -> UInt32
}

public enum AllowanceSlotAssignmentError: Error {
    case noSlotsAvailable
}
