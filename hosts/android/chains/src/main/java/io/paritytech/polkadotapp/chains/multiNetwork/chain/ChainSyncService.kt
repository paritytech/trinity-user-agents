package io.paritytech.polkadotapp.chains.multiNetwork.chain

import com.google.gson.Gson
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapExternalApisToLocal
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapRemoteAssetToLocal
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapRemoteChainToLocal
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapRemoteExplorersToLocal
import io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers.mapRemoteNodesToLocal
import io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.ChainFetcher
import io.paritytech.polkadotapp.common.utils.CollectionDiffer
import io.paritytech.polkadotapp.common.utils.retryUntilDone
import io.paritytech.polkadotapp.database.dao.ChainDao
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal.Companion.ENABLED_DEFAULT
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal
import io.paritytech.polkadotapp.database.utils.fullId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ChainSyncService @Inject constructor(
    private val chainDao: ChainDao,
    private val chainFetcher: ChainFetcher,
    private val gson: Gson,
) {
    suspend fun syncUp() =
        withContext(Dispatchers.Default) {
            val localChainsJoinedInfo = chainDao.getJoinChainInfo()
            val oldChains = localChainsJoinedInfo.map { it.chain }
            val oldAssets = localChainsJoinedInfo.flatMap { it.assets }
            val oldNodes = localChainsJoinedInfo.flatMap { it.nodes }
            val oldExplorers = localChainsJoinedInfo.flatMap { it.explorers }
            val oldExternalApis = localChainsJoinedInfo.flatMap { it.externalApis }

            val oldChainsById = oldChains.associateBy { it.id }
            val associatedOldAssets = oldAssets.associateBy { it.fullId() }

            val remoteChains = retryUntilDone { chainFetcher.getChains() }

            val newChains = remoteChains.map { mapRemoteChainToLocal(it, oldChainsById[it.chainId], gson) }
            val newAssets =
                remoteChains.flatMap { chain ->
                    chain.assets.map {
                        val fullAssetId = FullChainAssetIdLocal(chain.chainId, it.assetId)
                        val oldAsset = associatedOldAssets[fullAssetId]
                        mapRemoteAssetToLocal(chain, it, gson, oldAsset?.enabled ?: ENABLED_DEFAULT)
                    }
                }
            val newNodes = remoteChains.flatMap(::mapRemoteNodesToLocal)
            val newExplorers = remoteChains.flatMap(::mapRemoteExplorersToLocal)
            val newExternalApis = remoteChains.flatMap(::mapExternalApisToLocal)

            val chainsDiff = CollectionDiffer.findDiff(newChains, oldChains, forceUseNewItems = false)
            val assetDiff = CollectionDiffer.findDiff(newAssets, oldAssets, forceUseNewItems = false)
            val nodesDiff = CollectionDiffer.findDiff(newNodes, oldNodes, forceUseNewItems = false)
            val explorersDiff = CollectionDiffer.findDiff(newExplorers, oldExplorers, forceUseNewItems = false)
            val externalApisDiff = CollectionDiffer.findDiff(newExternalApis, oldExternalApis, forceUseNewItems = false)

            chainDao.applyDiff(
                chainDiff = chainsDiff,
                assetsDiff = assetDiff,
                nodesDiff = nodesDiff,
                explorersDiff = explorersDiff,
                externalApisDiff = externalApisDiff
            )
        }
}
