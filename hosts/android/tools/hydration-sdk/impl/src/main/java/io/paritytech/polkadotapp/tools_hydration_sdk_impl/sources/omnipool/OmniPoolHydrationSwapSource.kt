package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.orZero
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.observeNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.util.dynamicFees
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.chains.util.numberConstant
import io.paritytech.polkadotapp.chains.util.omnipool
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.common.utils.singleReplaySharedFlow
import io.paritytech.polkadotapp.common.utils.toMultiSubscription
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.exception.HydrationSwapQuoteException
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSource
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.fromHydrationAsset
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.DynamicFee
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.OmniPool
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.OmniPoolFees
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.OmniPoolToken
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.OmnipoolAssetState
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.RemoteIdAndLocalAsset
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.feeParamsConstant
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.quote
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.toExternalBalanceTypeSubscriptions
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import java.math.BigInteger
import javax.inject.Inject

class OmniPoolHydrationSwapSourceFactory @Inject constructor(
    @RemoteSourceQualifier
    private val remoteStorageSource: StorageDataSource,
    private val chainRegistry: ChainRegistry,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
) : HydrationSwapSource.Factory {
    override fun create(
        chain: Chain,
        overridableData: HydrationSwapOverridableData,
        weightSpec: WeightSpec
    ): HydrationSwapSource {
        return OmniPoolHydrationSwapSource(
            remoteStorageSource = remoteStorageSource,
            hydraDxAssetIdConverter = hydraDxAssetIdConverter,
            chain = chain,
            overridableData = overridableData,
            weightSpec = weightSpec,
            tokenBalanceTypeRegistry = tokenBalanceTypeRegistry,
            chainRegistry = chainRegistry
        )
    }
}

private class OmniPoolHydrationSwapSource(
    private val remoteStorageSource: StorageDataSource,
    private val chainRegistry: ChainRegistry,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val chain: Chain,
    private val weightSpec: WeightSpec,
    private val overridableData: HydrationSwapOverridableData,
) : HydrationSwapSource {
    private val pooledOnChainAssetIdsState: MutableSharedFlow<List<RemoteIdAndLocalAsset>> =
        singleReplaySharedFlow()

    private val omniPoolFlow: MutableSharedFlow<OmniPool> = singleReplaySharedFlow()

    override suspend fun sync() {
        val pooledOnChainAssetIds = getPooledOnChainAssetIds()

        val pooledChainAssetsIds = matchKnownChainAssetIds(pooledOnChainAssetIds)
        pooledOnChainAssetIdsState.emit(pooledChainAssetsIds)
    }

    override suspend fun availableSwapDirections(): Collection<HydrationSwapEdge> {
        val pooledOnChainAssetIds = pooledOnChainAssetIdsState.first()

        return pooledOnChainAssetIds.flatMap { remoteAndLocal ->
            pooledOnChainAssetIds.mapNotNull { otherRemoteAndLocal ->
                // In OmniPool, each asset is tradable with any other except itself
                if (remoteAndLocal.second.id != otherRemoteAndLocal.second.id) {
                    RealOmniPoolQuotingEdge(
                        fromAsset = remoteAndLocal,
                        toAsset = otherRemoteAndLocal
                    )
                } else {
                    null
                }
            }
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override suspend fun runSubscriptions(
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<Unit> {
        omniPoolFlow.resetReplayCache()

        val pooledAssets = pooledOnChainAssetIdsState.first()

        val omniPoolStateFlow = pooledAssets.map { (onChainId, _) ->
            remoteStorageSource.subscribe(chain.id, subscriptionBuilder) {
                metadata.omnipool.assets.observeNonNull(onChainId).map {
                    onChainId to it
                }
            }
        }
            .toMultiSubscription(pooledAssets.size)

        val poolAccountId = omniPoolAccountId()

        val omniPoolBalancesFlow = pooledAssets.map { (omniPoolTokenId, chainAsset) ->
            val tokenType = ExternalAssetId.fromHydrationAsset(chainAsset, omniPoolTokenId)

            tokenBalanceTypeRegistry.externalTypeFor(chain.id, tokenType)
                .subscribeTransferableBalance(
                    poolAccountId,
                    subscriptionBuilder,
                    overridableData.toExternalBalanceTypeSubscriptions()
                ).map { omniPoolTokenId to it }
        }
            .toMultiSubscription(pooledAssets.size)

        val feesFlow = pooledAssets.map { (omniPoolTokenId, _) ->
            remoteStorageSource.subscribe(chain.id, subscriptionBuilder) {
                metadata.dynamicFeesApi.assetFee.observe(omniPoolTokenId).map {
                    omniPoolTokenId to it
                }
            }
        }.toMultiSubscription(pooledAssets.size)

        val defaultFees = getDefaultFees()

        return combine(
            omniPoolStateFlow,
            omniPoolBalancesFlow,
            feesFlow
        ) { poolState, poolBalances, fees ->
            createOmniPool(poolState, poolBalances, fees, defaultFees)
        }
            .onEach(omniPoolFlow::emit)
            .map { }
    }

    private suspend fun getPooledOnChainAssetIds(): List<BigInteger> {
        return remoteStorageSource.query(chain.id) {
            val hubAssetId = metadata.omnipool().numberConstant("HubAssetId")
            val allAssets = runtime.metadata.omnipoolOrNull?.assets?.keys().orEmpty()

            // remove hubAssetId from trading paths
            allAssets.filter { it != hubAssetId }
        }
    }

    private suspend fun matchKnownChainAssetIds(onChainIds: List<HydraDxAssetId>): List<RemoteIdAndLocalAsset> {
        val hydraDxAssetIds = hydraDxAssetIdConverter.allOnChainIds(chain)

        return onChainIds.mapNotNull { onChainId ->
            val asset = hydraDxAssetIds[onChainId] ?: return@mapNotNull null

            onChainId to asset
        }
    }

    private fun createOmniPool(
        poolAssetStates: Map<HydraDxAssetId, OmnipoolAssetState>,
        poolBalances: Map<HydraDxAssetId, Balance>,
        fees: Map<HydraDxAssetId, DynamicFee?>,
        defaultFees: OmniPoolFees,
    ): OmniPool {
        val tokensState = poolAssetStates.mapValues { (tokenId, poolAssetState) ->
            val assetBalance = poolBalances[tokenId].orZero()
            val tokenFees =
                fees[tokenId]?.let { OmniPoolFees(it.protocolFee, it.assetFee) } ?: defaultFees

            OmniPoolToken(
                hubReserve = poolAssetState.hubReserve,
                shares = poolAssetState.shares,
                protocolShares = poolAssetState.protocolShares,
                tradeability = poolAssetState.tradeability,
                balance = assetBalance,
                fees = tokenFees
            )
        }

        return OmniPool(tokensState)
    }

    private suspend fun getDefaultFees(): OmniPoolFees {
        val runtime = chainRegistry.getRuntime(chain.id)

        val assetFeeParams =
            runtime.metadata.dynamicFees().feeParamsConstant("AssetFeeParameters", runtime)
        val protocolFeeParams =
            runtime.metadata.dynamicFees().feeParamsConstant("ProtocolFeeParameters", runtime)

        return OmniPoolFees(
            protocolFee = protocolFeeParams.minFee,
            assetFee = assetFeeParams.minFee
        )
    }

    private inner class RealOmniPoolQuotingEdge(
        val fromAsset: RemoteIdAndLocalAsset,
        val toAsset: RemoteIdAndLocalAsset,
    ) : HydrationSwapEdge {
        override val from: FullChainAssetId = fromAsset.second.fullId

        override val to: FullChainAssetId = toAsset.second.fullId

        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): Int {
            return weightSpec.highLiquidityPools
        }

        override suspend fun debugLabel(): String {
            return "OmniPool"
        }

        override suspend fun quote(amount: Balance, direction: SwapDirection): Balance {
            val omniPool = omniPoolFlow.first()

            return omniPool.quote(fromAsset.first, toAsset.first, amount, direction)
                ?: throw HydrationSwapQuoteException()
        }

        override fun routerPoolArgument(): DictEnum.Entry<*> {
            return DictEnum.Entry("Omnipool", null)
        }
    }
}
