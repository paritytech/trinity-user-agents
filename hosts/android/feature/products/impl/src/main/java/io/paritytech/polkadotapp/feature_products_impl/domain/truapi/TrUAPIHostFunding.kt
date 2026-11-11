package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionScreens
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.toActivityStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.onStart
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStatus
import uniffi.truapi.fundingStatusInFlight
import javax.inject.Inject
import kotlin.time.Instant
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** The host's funding entry points over the core, which holds the sessions and their history. */
class TrUAPIHostFunding internal constructor(
    private val runtime: FundingRuntime,
    private val sessionScreens: FundingSessionScreens,
    private val enabled: Boolean,
) : HostFunding {
    @Inject
    constructor(runtime: FundingRuntime, sessionScreens: FundingSessionScreens) :
        this(runtime, sessionScreens, FeatureOption.FUNDING.isEnabled)

    override suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?> =
        runtime.openFunding(direction.toCore(), amount?.toU128())

    override fun observeActivity(): Flow<FundingActivity> =
        if (!enabled) {
            flowOf(FundingActivity(inFlight = emptyList(), history = emptyList()))
        } else {
            combine(runtime.sessions, runtime.history) { sessions, history -> activityOf(sessions.values, history) }
                .onStart { runtime.start().logFailure("Failed to start the core for funding activity") }
        }

    override suspend fun acknowledgeShown(intents: List<String>) {
        intents.forEach { runtime.acknowledge(it).logFailure("Funding acknowledgement failed") }
    }

    override suspend fun showSession(intent: String) = sessionScreens.show(intent)
}

/** In flight while its status says so, newest opened first; history leaves those out, newest ended first. */
private fun activityOf(entries: Collection<FundingBookEntry>, history: List<FundingSessionSummary>): FundingActivity {
    val inFlight = entries.filter { fundingStatusInFlight(it.progress.status) }.sortedByDescending { it.session.openedAtMs }
    val flying = inFlight.mapTo(mutableSetOf()) { it.session.intent }
    return FundingActivity(
        inFlight = inFlight.map { it.toItem() },
        history = history.filter { it.intent !in flying }.sortedByDescending { it.ended.atMs }.map { it.toItem() },
    )
}

private fun FundingBookEntry.toItem(): FundingActivityItem {
    val quoted = session.choice?.quote?.let { if (session.direction == CoreFundingDirection.IN) it.receiveAmount else it.sendAmount }
    return FundingActivityItem(
        id = session.intent,
        direction = session.direction.toDomain(),
        rail = session.choice?.rail?.toType(),
        // In flight, only a released withdrawal has settled an amount.
        amount = ((session.stage as? FundingStage.Released)?.debited ?: session.amount ?: quoted)?.toBalance(),
        date = Instant.fromEpochMilliseconds(session.openedAtMs.toLong()),
        status = progress.status.toActivityStatus(),
    )
}

private fun FundingSessionSummary.toItem() = FundingActivityItem(
    id = intent,
    direction = direction.toDomain(),
    rail = choice?.rail?.toType(),
    amount = ended.cashAmount?.toBalance(),
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
