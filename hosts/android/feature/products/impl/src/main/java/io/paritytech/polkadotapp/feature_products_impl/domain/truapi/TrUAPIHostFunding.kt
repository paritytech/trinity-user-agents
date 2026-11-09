package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionPresenter
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.toActivityStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import uniffi.truapi.FundingActivityRow
import uniffi.truapi.FundingRail
import javax.inject.Inject
import kotlin.time.Instant
import uniffi.truapi.FundingActivity as CoreFundingActivity

/** The host's funding entry points over the core, which merges, sorts and hides the activity rows. */
class TrUAPIHostFunding @Inject constructor(
    private val runtime: FundingRuntime,
    private val sessionPresenter: FundingSessionPresenter,
) : HostFunding {
    override suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?> =
        runtime.openFunding(direction.toCore(), amount?.toU128())

    override fun observeActivity(): Flow<FundingActivity> = runtime.activity
        .onStart { runtime.start().logFailure("Failed to start the core for funding activity") }
        .filterNotNull()
        .map { it.toDomain() }

    override suspend fun acknowledgeShown(intents: List<String>) {
        if (intents.isEmpty()) return
        runtime.acknowledgeShown(intents).logFailure("Funding acknowledgement failed")
    }

    override suspend fun showSession(intent: String) = sessionPresenter.show(intent)
}

private fun CoreFundingActivity.toDomain() = FundingActivity(
    inFlight = inFlight.map { it.toItem() },
    history = history.map { it.toItem() },
)

private fun FundingActivityRow.toItem() = FundingActivityItem(
    id = intent,
    direction = direction.toDomain(),
    rail = rail?.toType(),
    amount = amount?.toBalance(),
    date = Instant.fromEpochMilliseconds(atMs.toLong()),
    status = status.toActivityStatus(),
)

private fun FundingRail.toType(): FundingRailType = when (this) {
    FundingRail.CARD -> FundingRailType.CARD
    FundingRail.BANK -> FundingRailType.BANK
    FundingRail.CRYPTO -> FundingRailType.CRYPTO
}
