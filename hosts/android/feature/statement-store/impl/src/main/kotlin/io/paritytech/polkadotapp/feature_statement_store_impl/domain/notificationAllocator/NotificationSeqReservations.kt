package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Slots promised to target accounts in this process from the moment their claim is scheduled until it is visible
 * on chain, so that neither a later allocation nor a parallel build hands the same slot to another account.
 */
@Singleton
class NotificationSeqReservations @Inject constructor() {
    private val slotByTarget = mutableMapOf<AccountId, NotificationSlot>()

    @Synchronized
    fun reserve(target: AccountId, slot: NotificationSlot): Result<Unit> {
        slotByTarget.values.removeIf { it.period < slot.period }

        val holder = slotByTarget.entries.firstOrNull { (account, reserved) -> reserved == slot && account != target }?.key
        if (holder != null) return Result.failure(NotificationSlotAlreadyReservedError(slot, holder, target))

        slotByTarget[target] = slot
        return Result.success(Unit)
    }

    @Synchronized
    fun release(target: AccountId) {
        slotByTarget.remove(target)
    }

    @Synchronized
    fun reservedFor(target: AccountId): NotificationSlot? = slotByTarget[target]

    /** Slots of [period] reserved for any account other than [exceptFor]. */
    @Synchronized
    fun reservedIn(period: UInt, exceptFor: AccountId?): Set<NotificationSlot> {
        return slotByTarget
            .filter { (account, slot) -> slot.period == period && account != exceptFor }
            .values
            .toSet()
    }
}

class NotificationSlotAlreadyReservedError(slot: NotificationSlot, holder: AccountId, requester: AccountId) :
    IllegalStateException("Notification slot $slot is already reserved for $holder, cannot reserve it for $requester")
