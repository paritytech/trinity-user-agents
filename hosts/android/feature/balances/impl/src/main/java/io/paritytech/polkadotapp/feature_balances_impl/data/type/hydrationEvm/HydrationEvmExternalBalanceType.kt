package io.paritytech.polkadotapp.feature_balances_impl.data.type.hydrationEvm

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalBalanceTypeSubscriptions
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.transferableBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Singleton

internal class HydrationEvmExternalBalanceType @AssistedInject constructor(
    @Assisted private val chainId: ChainId,
    @Assisted private val type: ExternalAssetId.HydrationEvm,
    private val hydrationEvmBalancePoller: HydrationEvmBalancePoller,
) : ExternalTokenBalanceType {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainId: ChainId, type: ExternalAssetId.HydrationEvm): HydrationEvmExternalBalanceType
    }

    override suspend fun subscribeTransferableBalance(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder,
        externalBalanceTypeSubscriptions: ExternalBalanceTypeSubscriptions?
    ): Flow<Balance> {
        return hydrationEvmBalancePoller.pollBalanceFlow(
            chainId = chainId,
            accountId = accountId,
            assetId = type.assetId,
            subscriptionBuilder = subscriptionBuilder,
            externalBalanceTypeSubscriptions = externalBalanceTypeSubscriptions
        )
            .map { it.balance.transferableBalance() }
    }
}
