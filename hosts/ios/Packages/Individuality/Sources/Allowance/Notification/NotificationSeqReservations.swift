import Foundation
import os
import SubstrateSdk

public enum NotificationSeqReservationError: Error, Equatable {
    case alreadyReserved(holder: AccountId)
}

/// Slots promised to target accounts in this process from the moment a claim is scheduled until it is
/// visible on chain, so that neither a later allocation nor a parallel build hands the same slot to another
/// account.
public final class NotificationSeqReservations: Sendable {
    private let slotByTarget = OSAllocatedUnfairLock<[AccountId: NotificationSlot.Key]>(initialState: [:])

    public init() {}

    func reserve(_ slot: NotificationSlot.Key, for target: AccountId) throws {
        try slotByTarget.withLock { reservations in
            reservations = reservations.filter { $0.value.period >= slot.period }

            if let holder = reservations.first(where: { $0.value == slot && $0.key != target })?.key {
                throw NotificationSeqReservationError.alreadyReserved(holder: holder)
            }

            reservations[target] = slot
        }
    }

    func release(_ target: AccountId) {
        slotByTarget.withLock { _ = $0.removeValue(forKey: target) }
    }

    func reserved(for target: AccountId) -> NotificationSlot.Key? {
        slotByTarget.withLock { $0[target] }
    }

    /// Slots of `period` reserved for any account other than `exceptFor`.
    func reserved(in period: UInt32, exceptFor target: AccountId?) -> Set<NotificationSlot.Key> {
        slotByTarget.withLock { reservations in
            Set(reservations.compactMap { account, slot in
                slot.period == period && account != target ? slot : nil
            })
        }
    }
}
