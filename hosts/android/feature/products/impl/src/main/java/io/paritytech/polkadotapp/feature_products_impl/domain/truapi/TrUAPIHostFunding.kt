package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionPresenter
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingActivityCenter
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class TrUAPIHostFunding @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val activityCenter: FundingActivityCenter,
    private val sessionPresenter: FundingSessionPresenter,
) : HostFunding {
    override suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?> =
        runtimeProvider.runtime().mapCatching { runtime ->
            runtime.openFunding(direction.toCore(), amount?.toU128())
        }

    override fun observeActivity(): Flow<FundingActivity> = activityCenter.observe()

    override suspend fun showSession(intent: String) = sessionPresenter.show(intent)
}
