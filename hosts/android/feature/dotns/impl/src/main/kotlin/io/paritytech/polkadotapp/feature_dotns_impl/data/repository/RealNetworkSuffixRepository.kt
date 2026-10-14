package io.paritytech.polkadotapp.feature_dotns_impl.data.repository

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_impl.data.blockchain.networkSuffix
import javax.inject.Inject

internal class RealNetworkSuffixRepository @Inject constructor(
    @param:RemoteSourceQualifier private val storageDataSource: StorageDataSource,
    private val chainRegistry: ChainRegistry,
) : NetworkSuffixRepository {
    override suspend fun networkSuffix(): Result<DotNsTld?> {
        return storageDataSource.queryCatching(chainRegistry.peopleChain().id) {
            runtime.metadata.networkSuffix.networkSuffix.query()
        }.map { it?.decodeToString()?.let(DotNsTld::parse) }
    }
}
