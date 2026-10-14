package io.paritytech.polkadotapp.tools_hydration_sdk_impl

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.novasama.substrate_sdk_android.runtime.extrinsic.BatchMode
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.findEvent
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.findEventOrThrow
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.chains.util.structOf
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.utils.flatMapAsync
import io.paritytech.polkadotapp.common.utils.flattenUnit
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.create
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mergeIfMultiple
import io.paritytech.polkadotapp.common.utils.withFlowScope
import io.paritytech.polkadotapp.feature_account_api.domain.model.toOriginCaller
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FastLookupCustomFeeCapability
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.metaAccountOrThrow
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.flatten
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapSdk
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.HydrationSwapDryRunOutcome
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapLimit
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapOutcome
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.toOnChainIdOrThrow
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.HydrationFastLookupCustomFeeCapabilityFactory
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.HydrationFeePayment
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSource
import kotlinx.coroutines.flow.Flow

internal class RealHydrationSwapSdk(
    override val chain: Chain,
    private val chainRegistry: ChainRegistry,
    private val dryRunApi: DryRunApi,
    private val weightSpec: WeightSpec,
    private val extrinsicService: ExtrinsicService,
    private val overridableData: HydrationSwapOverridableData,
    private val quotingFactories: Collection<HydrationSwapSource.Factory>,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val feePaymentFactory: HydrationFeePayment.Factory,
    private val sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    private val feeCapabilityFactory: HydrationFastLookupCustomFeeCapabilityFactory,
    private val signedOrigins: SignedOrigins
) : HydrationSwapSdk {
    private companion object {
        private const val ROUTE_EXECUTED_AMOUNT_OUT_IDX = 3
    }

    private val quotingSources: List<HydrationSwapSource> = createSources()

    private val directions = SingleValueCache {
        quotingSources.flatMapAsync { source ->
            source.availableSwapDirections()
        }
    }

    private val graph = SingleValueCache {
        Graph.create(directions())
    }

    override suspend fun sync(): Result<Unit> {
        return quotingSources.map {
            runCatching { it.sync() }
                .logFailure("Failed to execute sync of ${it::class.simpleName}")
        }.flattenUnit()
    }

    override suspend fun availableSwapDirections(): Collection<HydrationSwapEdge> {
        return directions()
    }

    override fun runSubscriptions(): Flow<Unit> {
        return withFlowScope { scope ->
            val subscriptionBuilder = sharedRequestsBuilderFactory.create(chain.id)

            quotingSources
                .map { it.runSubscriptions(subscriptionBuilder) }
                .also { subscriptionBuilder.subscribe(scope) }
                .mergeIfMultiple()
        }
    }

    override suspend fun submit(
        trade: Path<HydrationSwapEdge>,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset,
        origin: TransactionOrigin,
    ): Result<SwapOutcome> {
        return extrinsicService.submitExtrinsicAndAwaitExecution(
            chain = chain,
            origin = origin,
            options = ExtrinsicService.SubmissionOptions(
                batchMode = BatchMode.BATCH_ALL,
                feePayment = getFeePayment(feeAsset)
            )
        ) {
            executeRouterSwap(trade, swapLimit)
        }
            .flattenExecutionFailure()
            .map { it.emittedEvents.determineSwapOutcome() }
    }

    override suspend fun dryRun(
        trade: Path<HydrationSwapEdge>,
        swapLimit: SwapLimit,
        origin: TransactionOrigin
    ): Result<HydrationSwapDryRunOutcome> {
        return chainRegistry.withRuntime(chain.id) {
            dryRunApi.dryRunCall(
                originCaller = origin.metaAccountOrThrow().toOriginCaller(chain),
                call = executeRouterSwapCall(trade, swapLimit),
                chainId = chain.id,
            )
                .flatten()
                .mapCatching { dryRunEffects ->
                    val depositedAmount = dryRunEffects.emittedEvents.determineSwapOutcome()
                    HydrationSwapDryRunOutcome(depositedAmount.actualReceivedAmount)
                }
        }
    }

    override suspend fun estimateFee(
        trade: Path<HydrationSwapEdge>,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset
    ): Result<AccountFee> {
        return extrinsicService.estimateFee(
            chain = chain,
            options = ExtrinsicService.SubmissionOptions(
                batchMode = BatchMode.BATCH_ALL,
                feePayment = getFeePayment(feeAsset)
            ),
            origin = signedOrigins.wallet()
        ) {
            executeRouterSwap(trade, swapLimit)
        }
    }

    private fun List<GenericEvent.Instance>.determineSwapOutcome(): SwapOutcome {
        val swapExecutedEvent = findEvent(Modules.ROUTER, "RouteExecuted")
            ?: findEventOrThrow(Modules.ROUTER, "Executed")

        val amountOut = swapExecutedEvent.arguments[ROUTE_EXECUTED_AMOUNT_OUT_IDX]

        return SwapOutcome(
            actualReceivedAmount = bindBalance(amountOut)
        )
    }

    context(withRuntime: WithRuntime, builder: ExtrinsicBuilder)
    private suspend fun executeRouterSwap(
        trade: Path<HydrationSwapEdge>,
        actualSwapLimit: SwapLimit
    ) {
        builder.call(executeRouterSwapCall(trade, actualSwapLimit))
    }

    context(withRuntime: WithRuntime)
    private suspend fun executeRouterSwapCall(
        trade: Path<HydrationSwapEdge>,
        actualSwapLimit: SwapLimit
    ): GenericCall.Instance {
        return when (actualSwapLimit) {
            is SwapLimit.SpecifiedIn -> executeRouterSell(trade, actualSwapLimit)

            is SwapLimit.SpecifiedOut -> executeRouterBuy(trade, actualSwapLimit)
        }
    }

    context(withRuntime: WithRuntime)
    private suspend fun executeRouterBuy(
        trade: Path<HydrationSwapEdge>,
        limit: SwapLimit.SpecifiedOut,
    ): GenericCall.Instance {
        val firstSegment = trade.first()
        val lastSegment = trade.last()

        return composeCall(
            moduleName = Modules.ROUTER,
            callName = "buy",
            arguments = mapOf(
                "asset_in" to hydraDxAssetIdConverter.toOnChainIdOrThrow(firstSegment.from),
                "asset_out" to hydraDxAssetIdConverter.toOnChainIdOrThrow(lastSegment.to),
                "amount_out" to limit.amountOut.value,
                "max_amount_in" to limit.amountInMax.value,
                "route" to trade.routerTradePath()
            )
        )
    }

    context(withRuntime: WithRuntime)
    private suspend fun executeRouterSell(
        trade: Path<HydrationSwapEdge>,
        limit: SwapLimit.SpecifiedIn,
    ): GenericCall.Instance {
        val firstSegment = trade.first()
        val lastSegment = trade.last()

        return composeCall(
            moduleName = Modules.ROUTER,
            callName = "sell",
            arguments = mapOf(
                "asset_in" to hydraDxAssetIdConverter.toOnChainIdOrThrow(firstSegment.from),
                "asset_out" to hydraDxAssetIdConverter.toOnChainIdOrThrow(lastSegment.to),
                "amount_in" to limit.amountIn.value,
                "min_amount_out" to limit.amountOutMin.value,
                "route" to trade.routerTradePath()
            )
        )
    }

    private suspend fun List<HydrationSwapEdge>.routerTradePath(): List<Any?> {
        return map { segment ->
            structOf(
                "pool" to segment.routerPoolArgument(),
                "assetIn" to hydraDxAssetIdConverter.toOnChainIdOrThrow(segment.from),
                "assetOut" to hydraDxAssetIdConverter.toOnChainIdOrThrow(segment.to)
            )
        }
    }

    private suspend fun HydraDxAssetIdConverter.toOnChainIdOrThrow(localId: FullChainAssetId): HydraDxAssetId {
        val chainAsset = chain.assetsById.getValue(localId.assetId)

        return toOnChainIdOrThrow(chainAsset)
    }

    override suspend fun getFeePayment(feeAsset: Chain.Asset): FeePayment {
        return feePaymentFactory.create(
            asset = feeAsset,
            graph = graph::invoke,
            chain = chain,
        )
    }

    override suspend fun feeCapabilityLookup(chainId: ChainId): Result<FastLookupCustomFeeCapability> {
        require(chain.id == chainId) {
            "Chain id mismatch. Expected: ${chain.id}, Requested: $chainId"
        }

        return feeCapabilityFactory.create(chain)
    }

    private fun createSources(): List<HydrationSwapSource> {
        return quotingFactories.map { it.create(chain, overridableData, weightSpec) }
    }
}
