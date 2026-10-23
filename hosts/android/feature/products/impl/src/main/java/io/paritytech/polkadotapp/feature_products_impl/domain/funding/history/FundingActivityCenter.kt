package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_impl.data.repository.FundingHistoryRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toDomain
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Clock
import kotlin.time.Instant

/**
 * The CASH card's funding lists: sessions still in the core, then the ended ones the host keeps.
 *
 * An ended session is written to the host's store, then acknowledged. The core still keeps a released withdrawal
 * until its payout is reported, and a failed payout until its refund settles; such a session comes back
 * unacknowledged when the provider reports, and its row is rewritten by intent.
 *
 * Once per process the store is checked against every session the core says the host recorded, and a row lost from
 * the store is rebuilt from the core's summary.
 */
@Singleton
class FundingActivityCenter @Inject constructor(
    private val runtime: FundingRuntime,
    private val history: FundingHistoryRepository,
) {
    private val reloadMutex = Mutex()

    /** Whether lost rows were rebuilt from the core's history; read and set under [reloadMutex]. */
    private var historyRestored = false

    /** The earliest time a session in flight turns "taking longer", which no status change announces. */
    private val nextLonger = MutableStateFlow<Instant?>(null)

    /** Re-read on every session change, and when a session in flight turns "taking longer". */
    fun observe(): Flow<FundingActivity> =
        merge(runtime.sessionChanges().map { }, longerWakes())
            .onStart { emit(Unit) }
            .map { reloadMutex.withLock { reload() } }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun longerWakes(): Flow<Unit> = nextLonger.flatMapLatest { at ->
        if (at == null) {
            emptyFlow()
        } else {
            flow {
                delay(at - Clock.System.now())
                emit(Unit)
            }
        }
    }

    private suspend fun reload(): FundingActivity {
        val entries = runtime.sessions().map { it to runtime.progress(it.intent) }

        val inFlight = entries.mapNotNull { (session, progress) -> inFlightItem(session, progress) }
        val inFlightIds = inFlight.mapTo(mutableSetOf()) { it.id }
        nextLonger.value = entries
            .filter { (session, _) -> session.intent in inFlightIds }
            .mapNotNull { (_, progress) -> progress?.longerAtMs?.let { Instant.fromEpochMilliseconds(it.toLong()) } }
            .filter { it > Clock.System.now() }
            .minOrNull()

        settle(entries)
        if (!historyRestored) historyRestored = restoreHistory()
        val ended = entries.mapNotNull { (session, progress) -> FundingRecord.of(session, progress) }

        val stored = history.records().logFailure("Funding history read failed").getOrDefault(emptyList())
        val storedIds = stored.mapTo(mutableSetOf()) { it.intent }
        val records = (stored + ended.filter { it.intent !in storedIds }).filter { it.intent !in inFlightIds }

        return FundingActivity(
            inFlight = inFlight,
            history = records.sortedByDescending { it.settledAt }.map { it.toItem() },
        )
    }

    /** One row per ended session the host has not acknowledged yet; a failed write leaves it unacknowledged for next time. */
    private suspend fun settle(entries: List<Pair<FundingSession, FundingProgress?>>) {
        entries
            .filter { (session, _) -> !session.acknowledged }
            .forEach { (session, progress) ->
                val record = FundingRecord.of(session, progress) ?: return@forEach
                val saved = history.save(record).logFailure("Funding history write failed").isSuccess
                if (saved) runtime.acknowledge(record.intent).logFailure("Funding acknowledgement failed")
            }
    }

    /**
     * Writes a row for every session the core says the host recorded that the store lacks; a stored row wins. Returns
     * whether the core and the store could both be read, so a failed pass is tried again on the next reload.
     */
    private suspend fun restoreHistory(): Boolean {
        val recorded = runtime.history() ?: return false
        val stored = history.records().logFailure("Funding history read failed").getOrNull() ?: return false
        val storedIds = stored.mapTo(mutableSetOf()) { it.intent }
        recorded
            .filter { it.intent !in storedIds }
            .mapNotNull(FundingRecord::of)
            .forEach { history.save(it).logFailure("Funding history rebuild failed") }
        return true
    }

    /** The row of a session the core still has in progress, or `null` once its status is final. */
    private fun inFlightItem(session: FundingSession, progress: FundingProgress?): FundingActivityItem? {
        val status = progress?.status?.toActivityStatus() as? FundingActivityStatus.InProgress ?: return null
        val choice = session.choice
        val inbound = session.direction == FundingDirection.IN
        val debited = (session.stage as? FundingStage.Released)?.debited
        val quoted = choice?.quote?.let { if (inbound) it.receiveAmount else it.sendAmount }

        return FundingActivityItem(
            id = session.intent,
            direction = session.direction.toDomain(),
            rail = choice?.rail?.toType(),
            amount = (debited ?: session.amount ?: quoted)?.toBalance(),
            date = Instant.fromEpochMilliseconds(session.openedAtMs.toLong()),
            status = status,
        )
    }

    private fun FundingRecord.toItem() = FundingActivityItem(
        id = intent,
        direction = direction.toDomain(),
        rail = rail?.toType(),
        amount = settledAmount ?: requestedAmount,
        date = settledAt,
        status = when (outcome) {
            FundingRecordOutcome.Delivered -> FundingActivityStatus.ToppedUp
            FundingRecordOutcome.Released ->
                if (payout is FundingRecordPayout.Failed) FundingActivityStatus.PayoutFailed(refunded) else FundingActivityStatus.Sent

            FundingRecordOutcome.Refunded -> FundingActivityStatus.Refunded
            FundingRecordOutcome.Cancelled -> FundingActivityStatus.Cancelled(refunded)
            is FundingRecordOutcome.Failed -> FundingActivityStatus.Failed(outcome.reason)
        },
    )

    private fun FundingRail.toType(): FundingRailType = when (this) {
        FundingRail.CARD -> FundingRailType.CARD
        FundingRail.BANK -> FundingRailType.BANK
        FundingRail.CRYPTO -> FundingRailType.CRYPTO
    }
}
