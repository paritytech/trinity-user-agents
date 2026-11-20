import Foundation
import SubstrateOperation

public enum StatementStoreAllowanceError: Error, Equatable {
    /// Every evictable slot is still rate-limited by the chain; retry later.
    case noSlotsAvailable
    /// No occupied slot may be evicted, or none belongs to a key this installation holds.
    case noEvictableSlots
    case replacementRejected(InvalidTransaction)
}
