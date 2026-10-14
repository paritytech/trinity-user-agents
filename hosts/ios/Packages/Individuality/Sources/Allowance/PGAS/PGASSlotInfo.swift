import Foundation
import KeyDerivation

public struct PGASSlotInfo {
    public let day: UInt32
    public let slotIndex: UInt32
    public let personOrigin: PersonOrigin

    public init(
        day: UInt32,
        slotIndex: UInt32,
        personOrigin: PersonOrigin
    ) {
        self.day = day
        self.slotIndex = slotIndex
        self.personOrigin = personOrigin
    }
}
