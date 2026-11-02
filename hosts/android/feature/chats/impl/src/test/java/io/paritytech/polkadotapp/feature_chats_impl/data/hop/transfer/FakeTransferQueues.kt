@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.WithRetryState
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.yield
import kotlin.time.Duration
import kotlin.time.Instant

internal const val MAX_IN_FLIGHT = 10
private const val QUERY_TRIPWIRE = 1000

internal class FakeItem(val id: String, override val retryState: HopTransferRetryState) : WithRetryState

internal enum class RowStatus { PENDING, IN_PROGRESS, DONE, FAILED }

private class FakeRow(
    val id: String,
    val createdAt: Int,
    var status: RowStatus,
    var nextAttemptAtVirtualMs: Long?,
    var retryState: HopTransferRetryState,
    val behaviour: Behaviour
)

private sealed interface Behaviour {
    class Hang(val deferred: CompletableDeferred<Unit> = CompletableDeferred()) : Behaviour
    class Delay(val duration: Duration) : Behaviour
}

internal class FakeConcurrentQueue(private val scope: TestScope) : ConcurrentTransferQueue<FakeItem> {
    private val rows = linkedMapOf<String, FakeRow>()
    private val changes = MutableSharedFlow<Unit>(extraBufferCapacity = Int.MAX_VALUE)
    private val starts = mutableListOf<String>()
    private val returns = mutableMapOf<String, Long>()
    private val claimCallTimes = mutableListOf<Long>()
    private var backoffQueryCallCount = 0

    var spinDetected = false
        private set

    val claimCalls: Int get() = claimCallTimes.size
    val backoffQueryCalls: Int get() = backoffQueryCallCount
    val startedIds: List<String> get() = starts.toList()
    val inFlightCount: Int get() = starts.count { it !in returns }

    override val maxInFlight: Int = MAX_IN_FLIGHT

    var backoffQueryLatency: Duration = Duration.ZERO

    private val now: Long get() = scope.testScheduler.currentTime

    fun addHanging(
        vararg ids: String,
        nextAttemptAt: Duration? = null,
        status: RowStatus = RowStatus.PENDING,
        retryState: HopTransferRetryState = HopTransferRetryState.None
    ) {
        ids.forEach { add(it, nextAttemptAt, status, retryState, Behaviour.Hang()) }
    }

    fun addDelayed(id: String, duration: Duration) {
        add(id, nextAttemptAt = null, RowStatus.PENDING, HopTransferRetryState.None, Behaviour.Delay(duration))
    }

    fun emitChange() {
        changes.tryEmit(Unit)
    }

    fun complete(id: String) {
        (rows.getValue(id).behaviour as Behaviour.Hang).deferred.complete(Unit)
    }

    fun fail(id: String, error: Throwable) {
        (rows.getValue(id).behaviour as Behaviour.Hang).deferred.completeExceptionally(error)
    }

    fun cancel(id: String) {
        val row = rows.remove(id)
        (row?.behaviour as Behaviour.Hang).deferred.completeExceptionally(TransferCancelledException())
    }

    fun statusOf(id: String) = rows.getValue(id).status
    fun retryStateOf(id: String) = rows.getValue(id).retryState
    fun isInFlight(id: String) = id in starts && id !in returns
    fun returnedAt(id: String): Long? = returns[id]
    fun firstClaimCallAtOrAfter(time: Duration): Long? = claimCallTimes.firstOrNull { it >= time.inWholeMilliseconds }

    override suspend fun claimNextDue(): FakeItem? {
        yield()
        claimCallTimes += now
        tripIfSpinning(claimCallTimes.size)

        val row = rows.values
            .filter { it.status == RowStatus.PENDING }
            .filter { it.nextAttemptAtVirtualMs == null || it.nextAttemptAtVirtualMs!! <= now }
            .minByOrNull { it.createdAt }
            ?: return null

        row.status = RowStatus.IN_PROGRESS
        return FakeItem(row.id, row.retryState)
    }

    override suspend fun releaseInProgress() {
        rows.values.filter { it.status == RowStatus.IN_PROGRESS }.forEach { it.status = RowStatus.PENDING }
    }

    override fun queueChanges(): Flow<Unit> = changes

    override suspend fun nextBackoffEndAt(): Instant? {
        yield()
        delay(backoffQueryLatency)
        tripIfSpinning(++backoffQueryCallCount)

        return rows.values
            .filter { it.status == RowStatus.PENDING }
            .mapNotNull { it.nextAttemptAtVirtualMs }
            .minOrNull()
            ?.let(Instant::fromEpochMilliseconds)
    }

    override suspend fun process(item: FakeItem) {
        starts += item.id
        try {
            when (val behaviour = rows.getValue(item.id).behaviour) {
                is Behaviour.Hang -> behaviour.deferred.await()
                is Behaviour.Delay -> delay(behaviour.duration)
            }
        } finally {
            returns[item.id] = now
        }
    }

    override suspend fun markDone(item: FakeItem) = write(item.id) { status = RowStatus.DONE }

    override suspend fun markFailed(item: FakeItem, error: Throwable) = write(item.id) { status = RowStatus.FAILED }

    override suspend fun scheduleRetry(item: FakeItem, reschedule: RetryDecision.Reschedule) = write(item.id) {
        status = RowStatus.PENDING
        retryState = HopTransferRetryState(reschedule.attemptCount, reschedule.firstFailureAt, reschedule.nextAttemptAt)
        nextAttemptAtVirtualMs = reschedule.nextAttemptAt.toEpochMilliseconds()
    }

    private fun add(id: String, nextAttemptAt: Duration?, status: RowStatus, retryState: HopTransferRetryState, behaviour: Behaviour) {
        rows[id] = FakeRow(
            id = id,
            createdAt = rows.size,
            status = status,
            nextAttemptAtVirtualMs = nextAttemptAt?.let { now + it.inWholeMilliseconds },
            retryState = retryState,
            behaviour = behaviour
        )
    }

    private suspend fun tripIfSpinning(calls: Int) {
        if (calls > QUERY_TRIPWIRE) {
            spinDetected = true
            awaitCancellation()
        }
    }

    private suspend fun write(id: String, update: FakeRow.() -> Unit) {
        yield()
        rows[id]?.update()
        changes.tryEmit(Unit)
    }
}
