package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFundingSession
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingActivityCenter
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onSubscription
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import javax.inject.Inject

class TrUAPIHostFunding @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val bridge: AppFundingHostBridge,
    private val activityCenter: FundingActivityCenter,
) : HostFunding {
    override suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?> =
        runtimeProvider.runtime().mapCatching { runtime ->
            runtime.openFunding(direction.toCore(), amount?.toU128())
        }

    override fun observeSessions(): Flow<List<HostFundingSession>> = flow {
        val runtime = runtimeProvider.runtime()
            .logFailure("TrUAPI runtime unavailable; no funding sessions to show")
            .getOrNull()
            ?: return@flow emit(emptyList())

        emitAll(
            bridge.sessionChanges()
                .onSubscription { emit(Unit) }
                .map { runtime.fundingSessions().map { it.toDomain() } }
        )
    }

    override fun observeActivity(): Flow<FundingActivity> = activityCenter.observe()

    private fun FundingSession.toDomain() = HostFundingSession(
        intent = intent,
        direction = direction.toDomain(),
        amount = amount?.toBalance(),
        providerId = providerId?.let { ProductId.fromStoredValue(it) },
        inFlight = stage is FundingStage.Open,
    )
}
