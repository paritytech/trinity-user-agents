import Foundation

public enum StatementStoreAllowanceError: Error, Equatable {
    case noSlotsAvailable(secsToWait: TimeInterval)
    case noEvictableSlots
}
