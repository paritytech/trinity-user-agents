package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isOpen
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toDomain
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Instant

/**
 * The CASH card's funding lists: sessions still in flight, then the ended ones, which the core keeps.
 *
 * The ended list is read from the core's history each time it is built: on the first collection and whenever the
 * core pushes a session. Every ended session,
 * from this run or an earlier one, reads from its summary, which carries all its row and detail show.
 *
 * An ended session the core has not been told about is acknowledged once its row is built, since that row tells the
 * user how it went. The core keeps a released withdrawal until its payout is reported, and a failed payout until its
 * refund settles; such a session comes back unacknowledged when the provider reports, and is acknowledged again.
 */
@Singleton
class FundingActivityCenter @Inject constructor(
    private val runtime: FundingRuntime,
) {
    private val reloadMutex = Mutex()

    /**
     * The pushed sessions already acknowledged in this process, as pushed: one the core hands back after a late report
     * differs, so it is acknowledged again. Read and written under [reloadMutex].
     */
    private val acknowledged = mutableSetOf<FundingBookEntry>()

    /** Ended sessions whose history row was acknowledged as shown in this process. Under [reloadMutex]. */
    private val acknowledgedSummaries = mutableSetOf<String>()

    /**
     * Built again whenever the core pushes a session; never completes, so the collector stops it.
     * Collecting it starts the core, which then pushes them, and acknowledges each ended session once its row is
     * built, so the core stops pushing it on resume.
     */
    fun observe(): Flow<FundingActivity> =
        combine(runtime.sessions, runtime.history) { sessions, history -> sessions.values to history }
            .onStart { runtime.start().logFailure("Failed to start the core for funding activity") }
            .map { (sessions, history) -> reloadMutex.withLock { reload(sessions, history) } }

    /** The ended session [intent] as the user is shown it, or `null` when the core holds no ended session by that id. */
    suspend fun ended(intent: String): FundingRecord? =
        runtime.summary(intent)
            .logFailure("Funding summary read failed")
            .getOrNull()
            ?.let(FundingRecord::of)

    private suspend fun reload(sessions: Collection<FundingBookEntry>, history: List<FundingSessionSummary>): FundingActivity {
        val entries = sessions.sortedByDescending { it.session.openedAtMs }

        val inFlight = entries.mapNotNull { inFlightItem(it.session, it.progress) }
        val inFlightIds = inFlight.mapTo(mutableSetOf()) { it.id }

        val records = history
            .filter { it.intent !in inFlightIds }
            .mapNotNull(FundingRecord::of)

        acknowledgeEnded(entries)
        // A session the core holds in full is acknowledged by its entry above.
        acknowledgeShown(records.mapTo(mutableSetOf()) { it.intent } - entries.mapTo(mutableSetOf()) { it.session.intent })

        return FundingActivity(
            inFlight = inFlight,
            // The core leaves a cancel that moved no money out of history.
            history = records.sortedByDescending { it.settledAt }.map { it.toItem() },
        )
    }

    /** Acknowledges each ended session the core has not been told about; a failed call is tried again on the next reload. */
    private suspend fun acknowledgeEnded(entries: List<FundingBookEntry>) {
        entries
            .filter { !it.session.acknowledged && !it.session.stage.isOpen && it !in acknowledged }
            .forEach { entry ->
                runtime.acknowledge(entry.session.intent)
                    .logFailure("Funding acknowledgement failed")
                    .onSuccess { acknowledged += entry }
            }
    }

    /** Acknowledges the ended sessions whose history rows were built and not yet acknowledged in this process. */
    private suspend fun acknowledgeShown(intents: Set<String>) {
        val fresh = intents - acknowledgedSummaries
        if (fresh.isEmpty()) return
        fresh.forEach { intent ->
            runtime.acknowledge(intent)
                .logFailure("Funding acknowledgement failed")
                .onSuccess { acknowledgedSummaries += intent }
        }
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
