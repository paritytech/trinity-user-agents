package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_impl.data.repository.FundingHistoryRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isOpen
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toDomain
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
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
import uniffi.truapi.FundingStep
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Clock
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

/**
 * The CASH card's funding lists: sessions still in the core, then the ended ones the host keeps.
 *
 * An ended session is written to the host's store, then acknowledged, after which the core drops it. Outbound
 * value waits for its payout outcome, or a day, before it is acknowledged, and its row is rewritten if the
 * payout arrives in that time.
 */
@Singleton
class FundingActivityCenter @Inject constructor(
    private val runtime: FundingRuntime,
    private val history: FundingHistoryRepository,
) {
    private companion object {
        val REFRESH_INTERVAL = 10.seconds
    }

    private val reloadMutex = Mutex()

    /** Re-read on every session change, and on a timer so a step reached without a status change still shows. */
    fun observe(): Flow<FundingActivity> =
        merge(runtime.sessionChanges().map { }, ticker())
            .onStart { emit(Unit) }
            .map { reloadMutex.withLock { reload(Clock.System.now()) } }

    private fun ticker(): Flow<Unit> = flow {
        while (true) {
            delay(REFRESH_INTERVAL)
            emit(Unit)
        }
    }

    private suspend fun reload(now: Instant): FundingActivity {
        val entries = runtime.sessions().map { it to runtime.progress(it.intent) }

        val inFlight = entries
            .filter { (session, _) -> session.stage.isOpen }
            .map { (session, progress) -> inFlightItem(session, progress, now) }

        val ended = entries.mapNotNull { (session, progress) -> FundingRecord.of(session, progress) }
        settle(ended, now)

        val stored = history.records().logFailure("Funding history read failed").getOrDefault(emptyList())
        val storedIds = stored.mapTo(mutableSetOf()) { it.intent }
        val records = stored + ended.filter { it.intent !in storedIds }

        return FundingActivity(
            inFlight = inFlight,
            history = records.sortedByDescending { it.settledAt }.map { it.toItem() },
        )
    }

    /** One row per ended session, then the core may drop it; a failed write leaves it in the core for next time. */
    private suspend fun settle(ended: List<FundingRecord>, now: Instant) {
        ended.forEach { record ->
            val saved = history.save(record).logFailure("Funding history write failed").isSuccess
            if (saved && record.isFinal(now)) {
                runtime.acknowledge(record.intent).logFailure("Funding acknowledgement failed")
            }
        }
    }

    private fun inFlightItem(session: FundingSession, progress: FundingProgress?, now: Instant): FundingActivityItem {
        val step = step(session, progress)
        val choice = session.choice

        return FundingActivityItem(
            id = session.intent,
            direction = session.direction.toDomain(),
            rail = choice?.rail?.toType(),
            amount = (session.amount ?: choice?.quote?.let { if (session.direction == FundingDirection.IN) it.receiveAmount else it.sendAmount })
                ?.toBalance(),
            date = Instant.fromEpochMilliseconds(session.openedAtMs.toLong()),
            status = FundingActivityStatus.InProgress(
                step = step,
                delayed = step != FundingActivityStep.Retrying && isDelayed(session, progress, now),
            ),
        )
    }

    private fun step(session: FundingSession, progress: FundingProgress?): FundingActivityStep {
        if (progress?.retrying == true) return FundingActivityStep.Retrying

        val reached = progress?.steps.orEmpty().filter { it.reachedAtMs != null }.map { it.step }.toSet()
        val choice = session.choice

        return when (session.direction) {
            FundingDirection.IN -> when {
                FundingStep.CONVERSION in reached -> FundingActivityStep.Converting
                choice != null && choice.rail != FundingRail.CARD && FundingStep.PAYMENT !in reached ->
                    FundingActivityStep.WaitingForTransfer

                else -> FundingActivityStep.Upcoming
            }

            FundingDirection.OUT -> if (FundingStep.CONVERSION in reached && choice != null) {
                FundingActivityStep.ConvertingOut(choice.asset)
            } else {
                FundingActivityStep.TransactionInitiated
            }
        }
    }

    /** Slower than the provider said: the latest step is older than the time the chosen quote gave. */
    private fun isDelayed(session: FundingSession, progress: FundingProgress?, now: Instant): Boolean {
        val eta = session.choice?.quote?.etaSecs ?: return false
        val latestMs = progress?.steps.orEmpty().mapNotNull { it.reachedAtMs }.maxOrNull() ?: session.openedAtMs
        return now - Instant.fromEpochMilliseconds(latestMs.toLong()) > eta.toLong().seconds
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
                if (payout is FundingRecordPayout.Failed) FundingActivityStatus.PayoutFailed else FundingActivityStatus.Sent

            FundingRecordOutcome.Refunded -> FundingActivityStatus.Refunded
            is FundingRecordOutcome.Failed -> FundingActivityStatus.Failed
        },
    )

    private fun FundingRail.toType(): FundingRailType = when (this) {
        FundingRail.CARD -> FundingRailType.CARD
        FundingRail.BANK -> FundingRailType.BANK
        FundingRail.CRYPTO -> FundingRailType.CRYPTO
    }
}
