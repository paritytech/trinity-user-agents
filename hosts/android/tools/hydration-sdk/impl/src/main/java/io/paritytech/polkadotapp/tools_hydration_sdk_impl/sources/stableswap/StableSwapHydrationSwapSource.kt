package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap

import com.google.gson.Gson
import io.novasama.substrate_sdk_android.encrypt.json.asLittleEndianBytes
import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.orZero
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.common.utils.singleReplaySharedFlow
import io.paritytech.polkadotapp.common.utils.toMultiSubscription
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.exception.HydrationSwapQuoteException
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSource
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.HydrationAssetMetadataMap
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.assetRegistry
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.assets
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.RemoteAndLocalId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.RemoteAndLocalIdOptional
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.flatten
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.omniPoolAccountId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.StablePool
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.StablePoolAsset
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.StableSwapPoolInfo
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.StalbeSwapPoolPegInfo
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.quote
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.toExternalBalanceTypeSubscriptions
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import io.paritytech.polkadotapp.common.utils.combine as combine6

class StableSwapHydrationSwapSourceFactory @Inject constructor(
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val gson: Gson,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
) : HydrationSwapSource.Factory {
    override fun create(
        chain: Chain,
        overridableData: HydrationSwapOverridableData,
        weightSpec: WeightSpec
    ): HydrationSwapSource {
        return StableSwapHydrationSwapSource(
            remoteStorageSource = remoteStorageSource,
            hydraDxAssetIdConverter = hydraDxAssetIdConverter,
            chain = chain,
            gson = gson,
            overridableData = overridableData,
            weightSpec = weightSpec,
            tokenBalanceTypeRegistry = tokenBalanceTypeRegistry
        )
    }
}

private class StableSwapHydrationSwapSource(
    private val remoteStorageSource: StorageDataSource,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val gson: Gson,
    private val overridableData: HydrationSwapOverridableData,
    private val chain: Chain,
    private val weightSpec: WeightSpec,
) : HydrationSwapSource {
    private val initialPoolsInfo: MutableSharedFlow<Collection<PoolInitialInfo>> =
        singleReplaySharedFlow()

    private val stablePools: MutableSharedFlow<List<StablePool>> = singleReplaySharedFlow()

    override suspend fun sync() {
        val pools = getPools()

        val poolInitialInfo = pools.matchIdsWithLocal()
        initialPoolsInfo.emit(poolInitialInfo)
    }

    override suspend fun availableSwapDirections(): Collection<HydrationSwapEdge> {
        val poolInitialInfo = initialPoolsInfo.first()

        return poolInitialInfo.allPossibleDirections()
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override suspend fun runSubscriptions(
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<Unit> = coroutineScope {
        stablePools.resetReplayCache()

        val initialPoolsInfo = initialPoolsInfo.first()

        val poolInfoSubscriptions = initialPoolsInfo.map { poolInfo ->
            remoteStorageSource.subscribe(chain.id, subscriptionBuilder) {
                runtime.metadata.stableSwap.pools.observe(poolInfo.sharedAsset.first).map {
                    poolInfo.sharedAsset.first to it
                }
            }
        }
            .toMultiSubscription(initialPoolsInfo.size)

        val omniPoolAccountId = omniPoolAccountId()

        val allAssetIds = initialPoolsInfo.collectAllAssetIds()
        val assetsMetadataMap = fetchAssetMetadataMap(allAssetIds)

        val poolSharedAssetBalanceSubscriptions = initialPoolsInfo.map { poolInfo ->
            val sharedAssetRemoteId = poolInfo.sharedAsset.first

            subscribeTransferableBalance(
                subscriptionBuilder,
                omniPoolAccountId,
                sharedAssetRemoteId,
                assetsMetadataMap
            ).map {
                sharedAssetRemoteId to it
            }
        }.toMultiSubscription(initialPoolsInfo.size)

        val totalPooledAssets = initialPoolsInfo.sumOf { it.poolAssets.size }

        val poolParticipatingAssetsBalanceSubscription = initialPoolsInfo.flatMap { poolInfo ->
            val poolAccountId = stableSwapPoolAccountId(poolInfo.sharedAsset.first)

            poolInfo.poolAssets.map { poolAsset ->
                subscribeTransferableBalance(
                    subscriptionBuilder,
                    poolAccountId,
                    poolAsset.first,
                    assetsMetadataMap
                ).map {
                    val key = poolInfo.sharedAsset.first to poolAsset.first
                    key to it
                }
            }
        }.toMultiSubscription(totalPooledAssets)

        val totalIssuanceSubscriptions = initialPoolsInfo.map { poolInfo ->
            remoteStorageSource.subscribe(chain.id, subscriptionBuilder) {
                runtime.metadata.hydraTokens.totalIssuance.observe(poolInfo.sharedAsset.first).map {
                    poolInfo.sharedAsset.first to it.orZero()
                }
            }
        }.toMultiSubscription(initialPoolsInfo.size)

        val pegsSubscriptions = initialPoolsInfo.map { poolInfo ->
            remoteStorageSource.subscribe(chain.id, subscriptionBuilder) {
                val poolId = poolInfo.sharedAsset.first
                runtime.metadata.stableSwap.poolPegs.observe(poolId).map {
                    poolId to it
                }
            }
        }.toMultiSubscription(initialPoolsInfo.size)

        combine6(
            poolInfoSubscriptions,
            poolSharedAssetBalanceSubscriptions,
            poolParticipatingAssetsBalanceSubscription,
            totalIssuanceSubscriptions,
            overridableData.blockNumber(chain.id),
            pegsSubscriptions
        ) { poolInfos, poolSharedAssetBalances, poolParticipatingAssetBalances, totalIssuances, currentBlock, pegs ->
            createStableSwapPool(
                poolInfos,
                poolSharedAssetBalances,
                poolParticipatingAssetBalances,
                totalIssuances,
                currentBlock,
                assetsMetadataMap,
                pegs
            )
        }
            .onEach(stablePools::emit)
            .map { }
    }

    private suspend fun subscribeTransferableBalance(
        subscriptionBuilder: SharedRequestsBuilder,
        account: AccountId,
        assetId: HydraDxAssetId,
        assetMetadataMap: HydrationAssetMetadataMap,
    ): Flow<Balance> {
        // In case token type was not possible to resolve - just return zero
        val tokenType = assetMetadataMap.getAssetType(assetId) ?: return flowOf(Balance.ZERO)
        return tokenBalanceTypeRegistry.externalTypeFor(chain.id, tokenType)
            .subscribeTransferableBalance(
                account,
                subscriptionBuilder,
                overridableData.toExternalBalanceTypeSubscriptions()
            )
    }

    private fun createStableSwapPool(
        poolInfos: Map<HydraDxAssetId, StableSwapPoolInfo?>,
        poolSharedAssetBalances: Map<HydraDxAssetId, Balance>,
        poolParticipatingAssetBalances: Map<Pair<HydraDxAssetId, HydraDxAssetId>, Balance>,
        totalIssuances: Map<HydraDxAssetId, Balance>,
        currentBlock: BlockNumber,
        assetMetadataMap: HydrationAssetMetadataMap,
        pegs: Map<HydraDxAssetId, StalbeSwapPoolPegInfo?>
    ): List<StablePool> {
        return poolInfos.mapNotNull outer@{ (poolId, poolInfo) ->
            if (poolInfo == null) return@outer null

            val sharedAssetBalance = poolSharedAssetBalances[poolId].orZero()
            val sharedChainAssetPrecision =
                assetMetadataMap.getDecimals(poolId) ?: return@outer null
            val sharedAsset = StablePoolAsset(sharedAssetBalance, poolId, sharedChainAssetPrecision)
            val sharedAssetIssuance = totalIssuances[poolId].orZero()

            val pooledAssets = poolInfo.assets.mapNotNull { pooledAssetId ->
                val pooledAssetBalance =
                    poolParticipatingAssetBalances[poolId to pooledAssetId].orZero()
                val decimals = assetMetadataMap.getDecimals(pooledAssetId) ?: return@mapNotNull null

                StablePoolAsset(pooledAssetBalance, pooledAssetId, decimals)
            }

            StablePool(
                sharedAsset = sharedAsset,
                assets = pooledAssets,
                initialAmplification = poolInfo.initialAmplification,
                finalAmplification = poolInfo.finalAmplification,
                initialBlock = poolInfo.initialBlock,
                finalBlock = poolInfo.finalBlock,
                fee = poolInfo.fee,
                sharedAssetIssuance = sharedAssetIssuance,
                gson = gson,
                currentBlock = currentBlock,
                pegs = pegs[poolId]?.current ?: StablePool.getDefaultPegs(pooledAssets.size)
            )
        }
    }

    private fun Collection<PoolInitialInfo>.collectAllAssetIds(): List<HydraDxAssetId> {
        return flatMap { pool ->
            buildList {
                add(pool.sharedAsset.first)

                pool.poolAssets.onEach {
                    add(it.first)
                }
            }
        }
    }

    private suspend fun fetchAssetMetadataMap(allAssetIds: List<HydraDxAssetId>): HydrationAssetMetadataMap {
        return remoteStorageSource.query(chain.id) {
            val assetMetadatas = metadata.assetRegistry.assets.entries(allAssetIds).filterNotNull()
            HydrationAssetMetadataMap(
                nativeId = hydraDxAssetIdConverter.systemAssetId,
                metadataMap = assetMetadatas
            )
        }
    }

    private fun stableSwapPoolAccountId(poolId: HydraDxAssetId): AccountId {
        val prefix = "sts".encodeToByteArray()
        val suffix = poolId.toInt().asLittleEndianBytes()

        return (prefix + suffix).blake2b256().intoAccountId()
    }

    private suspend fun getPools(): Map<HydraDxAssetId, StableSwapPoolInfo> {
        return remoteStorageSource.query(chain.id) {
            runtime.metadata.stableSwapOrNull?.pools?.entries().orEmpty()
        }
    }

    private suspend fun Map<HydraDxAssetId, StableSwapPoolInfo>.matchIdsWithLocal(): List<PoolInitialInfo> {
        val allOnChainIds = hydraDxAssetIdConverter.allOnChainIds(chain)

        return mapNotNull outer@{ (poolAssetId, poolInfo) ->
            val poolAssetMatchedId = allOnChainIds[poolAssetId]?.fullId

            val participatingAssetsMatchedIds = poolInfo.assets.map { assetId ->
                val localId = allOnChainIds[assetId]?.fullId

                assetId to localId
            }

            PoolInitialInfo(
                sharedAsset = poolAssetId to poolAssetMatchedId,
                poolAssets = participatingAssetsMatchedIds
            )
        }
    }

    private fun Collection<PoolInitialInfo>.allPossibleDirections(): Collection<RealStableSwapQuotingEdge> {
        return flatMap { (poolAssetId, poolAssets) ->
            val allPoolAssetIds = buildList {
                addAll(poolAssets.mapNotNull { it.flatten() })

                val sharedAssetId = poolAssetId.flatten()

                if (sharedAssetId != null) {
                    add(sharedAssetId)
                }
            }

            allPoolAssetIds.flatMap { assetId ->
                allPoolAssetIds.mapNotNull { otherAssetId ->
                    otherAssetId.takeIf { assetId != otherAssetId }
                        ?.let {
                            RealStableSwapQuotingEdge(
                                assetId,
                                otherAssetId,
                                poolAssetId.first
                            )
                        }
                }
            }
        }
    }

    private data class PoolInitialInfo(
        val sharedAsset: RemoteAndLocalIdOptional,
        val poolAssets: List<RemoteAndLocalIdOptional>
    )

    inner class RealStableSwapQuotingEdge(
        val fromAsset: RemoteAndLocalId,
        val toAsset: RemoteAndLocalId,
        val poolId: HydraDxAssetId
    ) : HydrationSwapEdge {
        override val from: FullChainAssetId = fromAsset.second

        override val to: FullChainAssetId = toAsset.second

        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): Int {
            return weightSpec.stablePools
        }

        override suspend fun debugLabel(): String {
            val poolAsset = hydraDxAssetIdConverter.toChainAssetOrNull(chain, poolId)
            return if (poolAsset != null) {
                "StableSwap.${poolAsset.symbol}"
            } else {
                "StableSwap.$poolId"
            }
        }

        override suspend fun quote(amount: Balance, direction: SwapDirection): Balance {
            val allPools = stablePools.first()
            val relevantPool = allPools.first { it.sharedAsset.id == poolId }

            return relevantPool.quote(fromAsset.first, toAsset.first, amount, direction)
                ?: throw HydrationSwapQuoteException()
        }

        override fun routerPoolArgument(): DictEnum.Entry<*> {
            return DictEnum.Entry("Stableswap", poolId)
        }
    }
}
