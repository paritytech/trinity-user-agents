package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionScreens
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessions
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.toActivityStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.fundingStatusInFlight
import javax.inject.Inject
import kotlin.time.Instant
import uniffi.truapi.FundingDirection as CoreFundingDirection

class TrUAPIHostFunding @Inject constructor(
    private val sessions: FundingSessions,
    private val sessionScreens: FundingSessionScreens,
) : HostFunding {
    override suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?> =
        sessions.open(direction.toCore(), amount).map { it?.value }

    override suspend fun isFundingAvailable(): Result<Boolean> = sessions.isAvailable()

    override fun observeActivity(): Flow<FundingActivity> = sessions.entries().map(::activityOf)

    override suspend fun acknowledgeShown(intents: List<String>) {
        intents.forEach { sessions.acknowledge(FundingIntentId(it)) }
    }

    override suspend fun showSession(intent: String) = sessionScreens.show(FundingIntentId(intent))

    override suspend fun presentScreens() = sessionScreens.present()
}

/** In flight while its status says so, newest opened first; history leaves those out, newest ended first. */
private fun activityOf(entries: List<FundingEntry>): FundingActivity {
    val inFlight = entries.mapNotNull { entry ->
        val session = entry.session ?: return@mapNotNull null
        val progress = entry.progress?.takeIf { fundingStatusInFlight(it.status) } ?: return@mapNotNull null
        InFlight(session, progress)
    }.sortedByDescending { it.session.openedAtMs }
    val flying = inFlight.mapTo(mutableSetOf()) { it.session.intent }
    return FundingActivity(
        inFlight = inFlight.map { it.toItem() },
        history = entries.mapNotNull { it.summary }.filter { it.intent !in flying }.sortedByDescending { it.ended.atMs }.map { it.toItem() },
    )
}

private class InFlight(val session: FundingSession, val progress: FundingProgress)

private fun InFlight.toItem(): FundingActivityItem {
    val quoted = session.choice?.quote?.let { if (session.direction == CoreFundingDirection.IN) it.receiveAmount else it.sendAmount }
    return FundingActivityItem(
        id = session.intent,
        direction = session.direction.toDomain(),
        rail = session.choice?.rail?.toType(),
        // In flight, only a released withdrawal has settled an amount.
        amount = ((session.stage as? FundingStage.Released)?.debited ?: session.amount ?: quoted)?.toBalanceOrNull(),
        date = Instant.fromEpochMilliseconds(session.openedAtMs.toLong()),
        status = progress.status.toActivityStatus(),
    )
}

private fun FundingSessionSummary.toItem() = FundingActivityItem(
    id = intent,
    direction = direction.toDomain(),
    rail = choice?.rail?.toType(),
    amount = ended.cashAmount?.toBalanceOrNull(),
    date = Instant.fromEpochMilliseconds(ended.atMs.toLong()),
    // A cancel that still settled or refunded something reads as cancelled.
    status = if (status == FundingStatus.Failed(FundingFailure.Cancelled)) {
        FundingStatus.Cancelled(refunded)
    } else {
        status
    }.toActivityStatus(),
)

private fun FundingRail.toType(): FundingRailType = when (this) {
    FundingRail.CARD -> FundingRailType.CARD
    FundingRail.BANK -> FundingRailType.BANK
    FundingRail.CRYPTO -> FundingRailType.CRYPTO
}
