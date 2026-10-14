package io.paritytech.polkadotapp.feature_balances_impl.data.type.nativeType

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.orEmpty
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.typed.account
import io.paritytech.polkadotapp.chains.storage.typed.system
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalBalanceTypeSubscriptions
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalTokenBalanceType
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Singleton

internal class NativeExternalBalanceType @AssistedInject constructor(
    @Assisted private val chainId: ChainId,
    @RemoteSourceQualifier private val remoteStorageDataSource: StorageDataSource,
) : ExternalTokenBalanceType {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(chainId: ChainId): NativeExternalBalanceType
    }

    override suspend fun subscribeTransferableBalance(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder,
        externalBalanceTypeSubscriptions: ExternalBalanceTypeSubscriptions?
    ): Flow<Balance> {
        return remoteStorageDataSource.subscribe(chainId, subscriptionBuilder) {
            metadata.system.account.observe(accountId)
        }
            .map { it.orEmpty().transferableBalance() }
    }
}
