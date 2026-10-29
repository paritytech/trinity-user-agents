package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContext
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContextResolver
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.CurrentPeriodProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration

class RealNotificationStatementAccountAllocator @Inject constructor(
    private val durableTransactionService: DurableTransactionService,
    private val contextResolver: AllocateContextResolver,
    private val seqPicker: NotificationSeqPicker,
    private val reservations: NotificationSeqReservations,
    private val allocationLock: NotificationSlotAllocationLock,
    private val currentPeriodProvider: CurrentPeriodProvider,
) : NotificationStatementAccountAllocator {
    override fun currentPeriod(): UInt = currentPeriodProvider.current()

    override suspend fun initiateAllocation(target: AccountId): Result<Unit> {
        return initiateAllocations(listOf(target)).flatMap { initiated ->
            if (target in initiated) return@flatMap Result.success(Unit)

            Result.failure(NotificationAllocationError.NoFreeSlotInPeriod(target))
        }
    }

    override suspend fun initiateAllocations(targets: List<AccountId>): Result<List<AccountId>> {
        Timber.i("notificationAllocator: initiating allocation for ${targets.size} targets")

        return contextResolver.resolve()
            .flatMap { context -> allocationLock.withLock { scheduleWithinCapacity(context, targets) } }
            .onFailure { Timber.e(it, "Notification slot allocation failed for ${targets.size} targets") }
            .mapErrorNotInstance<_, NotificationAllocationError> { NotificationAllocationError.Unknown(targets, it) }
    }

    override suspend fun awaitAllocated(target: AccountId, timeout: Duration): Result<Unit> = runCancellableCatching {
        withTimeoutOrNull(timeout) {
            durableTransactionService.subscribeGroupStates(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup())
                .first { states -> states.anyArrived() || states.allGaveUp() }
        }
    }.flatMap { states -> awaitOutcome(target, timeout, states) }

    private suspend fun scheduleWithinCapacity(context: AllocateContext, targets: List<AccountId>): Result<List<AccountId>> {
        return filterClaimed(targets).flatMap { claimed -> scheduleUnclaimed(context, targets, claimed) }
    }

    private suspend fun scheduleUnclaimed(
        context: AllocateContext,
        targets: List<AccountId>,
        claimed: Set<AccountId>,
    ): Result<List<AccountId>> {
        val unclaimed = targets.filterNot { it in claimed }

        return seqPicker.freeSlots(context, forTarget = null).flatMap { free ->
            val assignments = unclaimed.zip(free)
            Timber.i(
                "notificationAllocator: period=${context.period}, already claimed=${claimed.size}, " +
                    "unclaimed=${unclaimed.size}, free slots=${free.size}; scheduling ${assignments.size}"
            )

            val scheduled = assignments.mapToSet { (target, _) -> target }
            scheduleClaims(assignments).map { targets.filter { it in claimed || it in scheduled } }
        }
    }

    private suspend fun filterClaimed(targets: List<AccountId>): Result<Set<AccountId>> {
        return targets
            .map { target -> hasClaim(target).map { claimed -> target.takeIf { claimed } } }
            .flattenResult()
            .map { it.filterNotNull().toSet() }
    }

    private suspend fun hasClaim(target: AccountId): Result<Boolean> {
        return durableTransactionService.getGroupStates(NOTIFICATION_SLOT_DOMAIN, target.notificationSlotGroup())
            .map { states -> states.any { it.status.canArrive } }
    }

    private suspend fun scheduleClaims(assignments: List<Pair<AccountId, NotificationSlot>>): Result<Unit> {
        return assignments
            .map { (target, slot) -> scheduleClaim(target, slot) }
            .flattenResult()
            .coerceToUnit()
    }

    // Reserved before the claim is recorded: the claim is built later, and until then the slot is only ours here.
    private suspend fun scheduleClaim(target: AccountId, slot: NotificationSlot): Result<Unit> {
        Timber.i("notificationAllocator: scheduling claim for $target on seq=${slot.seq} in ${slot.collection}")

        return reservations.reserve(target, slot).flatMap {
            durableTransactionService.schedule(
                domain = NOTIFICATION_SLOT_DOMAIN,
                groupId = target.notificationSlotGroup(),
                policies = listOf(target.notificationSlotPolicy()),
                onRegister = {},
            )
                .onFailure { reservations.release(target) }
                .coerceToUnit()
        }
    }

    private fun awaitOutcome(target: AccountId, timeout: Duration, states: List<DurableTxState>?): Result<Unit> = when {
        states == null -> {
            Timber.w("notificationAllocator: claim for $target did not land within $timeout")
            Result.failure(NotificationAllocationError.Timeout(target, timeout))
        }
        states.anyArrived() -> {
            Timber.i("notificationAllocator: claim for $target executed")
            Result.success(Unit)
        }
        else -> {
            Timber.w("notificationAllocator: claim for $target gave up, no free slot")
            Result.failure(NotificationAllocationError.NoFreeSlotInPeriod(target))
        }
    }

    private fun List<DurableTxState>.anyArrived(): Boolean = any { it.status.isArrived }

    private fun List<DurableTxState>.allGaveUp(): Boolean = isNotEmpty() && none { it.status.canArrive }
}
