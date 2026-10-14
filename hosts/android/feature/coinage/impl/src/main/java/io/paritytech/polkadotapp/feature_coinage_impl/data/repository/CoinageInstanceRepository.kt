package io.paritytech.polkadotapp.feature_coinage_impl.data.repository

import io.paritytech.polkadotapp.chains.di.LocalSourceQualifier
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.observeNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.subscribeCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.instances
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainInstanceRecord
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.first
import java.math.BigInteger
import javax.inject.Inject

interface CoinageInstanceRepository {
    suspend fun assetUnit(instanceId: CoinageInstanceId): Result<BigInteger>
}

class RealCoinageInstanceRepository @Inject constructor(
    @param:LocalSourceQualifier private val localStorageSource: StorageDataSource,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) : CoinageInstanceRepository {
    override suspend fun assetUnit(instanceId: CoinageInstanceId): Result<BigInteger> {
        return localStorageSource.subscribeCatching(chainAssetProvider.chainId()) {
            metadata.coinage.instances.observeNonNull(instanceId.toLong().toBigInteger())
        }.first().map(OnChainInstanceRecord::assetUnit)
    }
}
