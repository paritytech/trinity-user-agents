package io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.flatMap
import kotlin.time.Duration

/** Allowance one notification binding grants: a single statement of at most this encoded size. */
val NOTIFICATION_STATEMENT_MAX_SIZE: InformationSize = 10.kilobytes

/**
 * Funds statement accounts from anonymous notification slots: each binding lets its account keep one statement of
 * up to [NOTIFICATION_STATEMENT_MAX_SIZE] until the current period ends plus the runtime's grace window. An account
 * can hold one live binding, so callers pass a fresh account per period.
 */
interface NotificationStatementAccountAllocator {
    /** The period slots are claimed in right now; a binding made now lives until this period ends plus grace. */
    fun currentPeriod(): UInt

    /**
     * Reserves a free slot of the current period for [target] and schedules its durable claim. A no-op when a claim
     * for [target] is already in flight or has landed. Returns once the claim is recorded, not when it lands.
     */
    suspend fun initiateAllocation(target: AccountId): Result<Unit>

    /**
     * Like [initiateAllocation] for each of [targets], in the given order of preference: when fewer slots are free
     * than targets, only the leading ones are scheduled. Returns the targets that were scheduled or already claimed.
     */
    suspend fun initiateAllocations(targets: List<AccountId>): Result<List<AccountId>>

    /**
     * Suspends until a claim for [target] has executed (pending finality is enough), up to [timeout].
     *
     * Fails with [NotificationAllocationError.NoFreeSlotInPeriod] when the claim gave up because no slot was free,
     * and with [NotificationAllocationError.Timeout] when [timeout] elapses first.
     */
    suspend fun awaitAllocated(target: AccountId, timeout: Duration): Result<Unit>
}

/** [NotificationStatementAccountAllocator.initiateAllocation] followed by [NotificationStatementAccountAllocator.awaitAllocated]. */
suspend fun NotificationStatementAccountAllocator.allocate(target: AccountId, timeout: Duration): Result<Unit> {
    return initiateAllocation(target).flatMap { awaitAllocated(target, timeout) }
}

sealed class NotificationAllocationError(message: String, cause: Throwable?) : Throwable(message, cause) {
    class NoFreeSlotInPeriod(target: AccountId) :
        NotificationAllocationError("No free notification slot in the current period for $target", null)

    class Timeout(target: AccountId, timeout: Duration) :
        NotificationAllocationError("Notification slot claim for $target did not land within $timeout", null)

    class Unknown(targets: List<AccountId>, cause: Throwable) :
        NotificationAllocationError("Notification slot allocation failed for $targets", cause)
}
