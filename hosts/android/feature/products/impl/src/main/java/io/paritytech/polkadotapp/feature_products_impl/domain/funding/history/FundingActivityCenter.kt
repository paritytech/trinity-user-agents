package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toDomain
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.onStart
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
 * core pushes a session. Every ended session, from this run or an earlier one, reads from its summary, which carries
 * all its row and detail show.
 *
 * Reading the lists changes nothing in the core. The screen showing the history calls [acknowledgeShown] for the
 * ended rows it shows, since those tell the user how each session went.
 */
@Singleton
class FundingActivityCenter @Inject constructor(
    private val runtime: FundingRuntime,
) {
    /** Built again whenever the core pushes a session; never completes. Collecting it starts the core. */
    fun observe(): Flow<FundingActivity> =
        combine(runtime.sessions, runtime.history) { sessions, history -> activityOf(sessions.values, history) }
            .onStart { runtime.start().logFailure("Failed to start the core for funding activity") }

    /**
     * Records that the user saw how the ended sessions [intents] went. Until then the core keeps each one owed to the
     * host; one it hands back after a late payout or refund report is owed again.
     */
    suspend fun acknowledgeShown(intents: Collection<String>) {
        intents.forEach { runtime.acknowledge(it).logFailure("Funding acknowledgement failed") }
    }

    /** The ended session [intent] as the user is shown it, or `null` when the core holds no ended session by that id. */
    suspend fun ended(intent: String): FundingRecord? =
        runtime.summary(intent)
            .logFailure("Funding summary read failed")
            .getOrNull()
            ?.let(FundingRecord::of)

    private fun activityOf(sessions: Collection<FundingBookEntry>, history: List<FundingSessionSummary>): FundingActivity {
        val entries = sessions.sortedByDescending { it.session.openedAtMs }

        val inFlight = entries.mapNotNull { inFlightItem(it.session, it.progress) }
        val inFlightIds = inFlight.mapTo(mutableSetOf()) { it.id }

        val records = history
            .filter { it.intent !in inFlightIds }
            .mapNotNull(FundingRecord::of)

        return FundingActivity(
            inFlight = inFlight,
            // The core leaves a cancel that moved no money out of history.
            history = records.sortedByDescending { it.settledAt }.map { it.toItem() },
        )
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
