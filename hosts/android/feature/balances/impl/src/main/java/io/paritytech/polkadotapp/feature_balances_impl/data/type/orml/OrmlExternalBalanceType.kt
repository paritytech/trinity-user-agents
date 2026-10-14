package io.paritytech.polkadotapp.feature_balances_impl.data.type.orml

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalBalanceTypeSubscriptions
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalTokenBalanceType
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.api.accounts
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.api.tokens
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.orEmpty
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.transferableBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Singleton

internal class OrmlExternalBalanceType @AssistedInject constructor(
    @Assisted private val chainId: ChainId,
    @Assisted private val type: ExternalAssetId.Orml,
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
) : ExternalTokenBalanceType {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainId: ChainId, type: ExternalAssetId.Orml): OrmlExternalBalanceType
    }

    override suspend fun subscribeTransferableBalance(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder,
        externalBalanceTypeSubscriptions: ExternalBalanceTypeSubscriptions?
    ): Flow<Balance> {
        return remoteStorageSource.subscribe(chainId, subscriptionBuilder) {
            metadata.tokens.accounts.observe(accountId, type.currencyId)
        }
            .map { assetAccount -> assetAccount.orEmpty().transferableBalance() }
    }
}
