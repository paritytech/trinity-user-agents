package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.aave

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.common.data.substrate.castToList
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.common.utils.singleReplaySharedFlow
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.exception.HydrationSwapQuoteException
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSource
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.aave.model.AavePool
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.aave.model.AavePools
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.RemoteAndLocalId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.matchId
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import timber.log.Timber
import javax.inject.Inject

class AaveHydrationSwapSourceFactory @Inject constructor(
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
) : HydrationSwapSource.Factory {
    override fun create(
        chain: Chain,
        overridableData: HydrationSwapOverridableData,
        weightSpec: WeightSpec
    ): HydrationSwapSource {
        return AaveHydrationSwapSource(
            hydraDxAssetIdConverter = hydraDxAssetIdConverter,
            multiChainRuntimeCallsApi = multiChainRuntimeCallsApi,
            chain = chain,
            overridableData = overridableData,
            weightSpec = weightSpec
        )
    }
}

private class AaveHydrationSwapSource(
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
    private val chain: Chain,
    private val overridableData: HydrationSwapOverridableData,
    private val weightSpec: WeightSpec,
) : HydrationSwapSource {
    private val initialPoolsInfo: MutableSharedFlow<Collection<AavePoolInitialInfo>> = singleReplaySharedFlow()

    private val aavePools: MutableSharedFlow<AavePools> = singleReplaySharedFlow()

    override suspend fun sync() {
        val pairs = getPairs()

        val poolInitialInfo = pairs.matchIdsWithLocal()
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
        aavePools.resetReplayCache()

        overridableData.blockNumber(chain.id).map {
            val pools = getPools()
            aavePools.emit(pools)
        }
    }

    private suspend fun getPairs(): List<AavePoolPair> {
        return runCatching {
            multiChainRuntimeCallsApi.forChain(chain.id).call(
                section = "AaveTradeExecutor",
                method = "pairs",
                arguments = emptyMap(),
                returnBinding = ::bindPairs
            )
        }
            .onFailure { Timber.w(it, "Failed to get aave pairs") }
            .getOrDefault(emptyList())
    }

    private suspend fun getPools(): AavePools {
        return multiChainRuntimeCallsApi.forChain(chain.id).call(
            section = "AaveTradeExecutor",
            method = "pools",
            arguments = emptyMap(),
            returnBinding = ::bindPools
        )
    }

    private suspend fun List<AavePoolPair>.matchIdsWithLocal(): List<AavePoolInitialInfo> {
        val allOnChainIds = hydraDxAssetIdConverter.allOnChainIds(chain)

        return mapNotNull { poolInfo ->
            AavePoolInitialInfo(
                firstAsset = allOnChainIds.matchId(poolInfo.firstAsset) ?: return@mapNotNull null,
                secondAsset = allOnChainIds.matchId(poolInfo.secondAsset) ?: return@mapNotNull null,
            )
        }
    }

    private fun Collection<AavePoolInitialInfo>.allPossibleDirections(): Collection<RealAaveSwapQuotingEdge> {
        return buildList {
            this@allPossibleDirections.forEach { poolInfo ->
                add(RealAaveSwapQuotingEdge(fromAsset = poolInfo.firstAsset, toAsset = poolInfo.secondAsset))
                add(RealAaveSwapQuotingEdge(fromAsset = poolInfo.secondAsset, toAsset = poolInfo.firstAsset))
            }
        }
    }

    inner class RealAaveSwapQuotingEdge(
        val fromAsset: RemoteAndLocalId,
        val toAsset: RemoteAndLocalId,
    ) : HydrationSwapEdge {
        override val from: FullChainAssetId = fromAsset.second

        override val to: FullChainAssetId = toAsset.second

        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): Int {
            return weightSpec.stablePools
        }

        override suspend fun debugLabel(): String {
            return "Aave"
        }

        override suspend fun quote(amount: Balance, direction: SwapDirection): Balance {
            val allPools = aavePools.first()

            return allPools.quote(fromAsset.first, toAsset.first, amount, direction)
                ?: throw HydrationSwapQuoteException()
        }

        override fun routerPoolArgument(): DictEnum.Entry<*> {
            return DictEnum.Entry("Aave", null)
        }
    }

    private fun bindPairs(decoded: Any?): List<AavePoolPair> {
        return bindList(decoded) { item ->
            val (first, second) = item.castToList()
            AavePoolPair(bindNumber(first), bindNumber(second))
        }
    }

    private fun bindPools(decoded: Any?): AavePools {
        val pools = bindList(decoded, ::bindPool)
        return AavePools(pools)
    }

    private fun bindPool(decoded: Any?): AavePool {
        val asStruct = decoded.castToStruct()

        return AavePool(
            reserve = bindNumber(asStruct["reserve"]),
            atoken = bindNumber(asStruct["atoken"]),
            liqudityIn = bindBalance(asStruct["liqudityIn"]),
            liquidityOut = bindBalance(asStruct["liqudityOut"])
        )
    }
}

private class AavePoolPair(val firstAsset: HydraDxAssetId, val secondAsset: HydraDxAssetId)

private class AavePoolInitialInfo(
    val firstAsset: RemoteAndLocalId,
    val secondAsset: RemoteAndLocalId
)
