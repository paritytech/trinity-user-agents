package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.util.xyk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.combine
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.common.utils.singleReplaySharedFlow
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.exception.HydrationSwapQuoteException
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSource
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.fromHydrationAsset
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.RemoteAndLocalId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.localId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.matchId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.remoteId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model.XYKPool
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model.XYKPoolAsset
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model.XYKPoolInfo
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model.XYKPools
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model.poolFeesConstant
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.toExternalBalanceTypeSubscriptions
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class XYKHydrationSwapSourceFactory @Inject constructor(
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
) : HydrationSwapSource.Factory {
    override fun create(
        chain: Chain,
        overridableData: HydrationSwapOverridableData,
        weightSpec: WeightSpec
    ): HydrationSwapSource {
        return XYKHydrationSwapSource(
            remoteStorageSource = remoteStorageSource,
            hydraDxAssetIdConverter = hydraDxAssetIdConverter,
            chain = chain,
            overridableData = overridableData,
            weightSpec = weightSpec,
            tokenBalanceTypeRegistry = tokenBalanceTypeRegistry
        )
    }
}

private class XYKHydrationSwapSource(
    private val remoteStorageSource: StorageDataSource,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val overridableData: HydrationSwapOverridableData,
    private val chain: Chain,
    private val weightSpec: WeightSpec,
) : HydrationSwapSource {
    private val initialPoolsInfo: MutableSharedFlow<Collection<PoolInitialInfo>> = singleReplaySharedFlow()

    private val xykPools: MutableSharedFlow<XYKPools> = singleReplaySharedFlow()

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
        xykPools.resetReplayCache()

        val initialPoolsInfo = initialPoolsInfo.first()

        val poolsSubscription = initialPoolsInfo.map { poolInfo ->
            val firstBalanceFlow = subscribeToBalance(poolInfo.firstAsset, poolInfo.poolAddress, subscriptionBuilder)
            val secondBalanceFlow = subscribeToBalance(poolInfo.secondAsset, poolInfo.poolAddress, subscriptionBuilder)

            firstBalanceFlow.combine(secondBalanceFlow) { firstBalance, secondBalance ->
                XYKPool(
                    address = poolInfo.poolAddress,
                    firstAsset = XYKPoolAsset(firstBalance, poolInfo.firstAsset.first),
                    secondAsset = XYKPoolAsset(secondBalance, poolInfo.secondAsset.first),
                )
            }
        }.combine()

        val fees = remoteStorageSource.query(chain.id) {
            runtime.metadata.xyk().poolFeesConstant(runtime)
        }

        poolsSubscription.map { pools ->
            val built = XYKPools(fees, pools)
            xykPools.emit(built)
        }
    }

    private suspend fun subscribeToBalance(
        assetId: RemoteAndLocalId,
        poolAddress: AccountId,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<Balance> {
        val chainAsset = chain.assetsById.getValue(assetId.localId.assetId)
        val tokenType = ExternalAssetId.fromHydrationAsset(chainAsset, assetId.remoteId)

        return tokenBalanceTypeRegistry.externalTypeFor(chain.id, tokenType)
            .subscribeTransferableBalance(
                poolAddress,
                subscriptionBuilder,
                overridableData.toExternalBalanceTypeSubscriptions()
            )
    }

    private suspend fun getPools(): Map<AccountId, XYKPoolInfo> {
        return remoteStorageSource.query(chain.id) {
            runtime.metadata.xykOrNull?.poolAssets?.entries().orEmpty()
        }
    }

    private suspend fun Map<AccountId, XYKPoolInfo>.matchIdsWithLocal(): List<PoolInitialInfo> {
        val allOnChainIds = hydraDxAssetIdConverter.allOnChainIds(chain)

        return mapNotNull { (poolAddress, poolInfo) ->
            PoolInitialInfo(
                poolAddress = poolAddress,
                firstAsset = allOnChainIds.matchId(poolInfo.firstAsset) ?: return@mapNotNull null,
                secondAsset = allOnChainIds.matchId(poolInfo.secondAsset) ?: return@mapNotNull null,
            )
        }
    }

    private fun Collection<PoolInitialInfo>.allPossibleDirections(): Collection<RealXYKSwapQuotingEdge> {
        return buildList {
            this@allPossibleDirections.forEach { poolInfo ->
                add(
                    RealXYKSwapQuotingEdge(
                        fromAsset = poolInfo.firstAsset,
                        toAsset = poolInfo.secondAsset,
                        poolAddress = poolInfo.poolAddress
                    )
                )

                add(
                    RealXYKSwapQuotingEdge(
                        fromAsset = poolInfo.secondAsset,
                        toAsset = poolInfo.firstAsset,
                        poolAddress = poolInfo.poolAddress
                    )
                )
            }
        }
    }

    inner class RealXYKSwapQuotingEdge(
        val fromAsset: RemoteAndLocalId,
        val toAsset: RemoteAndLocalId,
        val poolAddress: AccountId
    ) : HydrationSwapEdge {
        override val from: FullChainAssetId = fromAsset.second

        override val to: FullChainAssetId = toAsset.second

        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): Int {
            return weightSpec.lowLiquidityPools
        }

        override suspend fun debugLabel(): String {
            return "XYK"
        }

        override suspend fun quote(amount: Balance, direction: SwapDirection): Balance {
            val allPools = xykPools.first()

            return allPools.quote(poolAddress, fromAsset.first, toAsset.first, amount, direction)
                ?: throw HydrationSwapQuoteException()
        }

        override fun routerPoolArgument(): DictEnum.Entry<*> {
            return DictEnum.Entry("XYK", null)
        }
    }
}

private class PoolInitialInfo(
    val poolAddress: AccountId,
    val firstAsset: RemoteAndLocalId,
    val secondAsset: RemoteAndLocalId
)
