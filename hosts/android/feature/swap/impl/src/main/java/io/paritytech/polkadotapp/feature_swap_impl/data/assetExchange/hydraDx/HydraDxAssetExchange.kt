package io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.hydraDx

import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.graph.Edge
import io.paritytech.polkadotapp.common.utils.graph.EdgeWeight
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.common.utils.invoke
import io.paritytech.polkadotapp.common.utils.lazyAsync
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperation
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationPrototype
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationSubmissionArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.ReQuoteTrigger
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapExecutionOutcome
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapLimit
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapMaxAdditionalAmountDeduction
import io.paritytech.polkadotapp.feature_swap_api.domain.model.UsdConverter
import io.paritytech.polkadotapp.feature_swap_api.domain.model.createAggregated
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.AssetExchange
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.FeePaymentProviderOverride
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.ParentQuoterArgs
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.SharedSwapSubscriptions
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.SwapWeights
import io.paritytech.polkadotapp.feature_swap_impl.domain.AssetInAdditionalSwapDeductionUseCase
import io.paritytech.polkadotapp.feature_swap_impl.domain.fee.SubmissionOnlyAtomicSwapOperationFee
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapSdk
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import kotlinx.coroutines.flow.Flow
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.collections.isNotEmpty
import kotlin.time.Duration
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection as HydrationSwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapLimit as HydrationSwapLimit

class HydraDxAssetExchangeFactory @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
    private val swapDeductionUseCase: AssetInAdditionalSwapDeductionUseCase,
    private val hydrationSwapSdkFactory: HydrationSwapSdk.Factory,
) : AssetExchange.SingleChainFactory {
    override suspend fun create(chain: Chain, swapHost: AssetExchange.SwapHost): AssetExchange {
        return HydraDxAssetExchange(chain, swapHost, chainStateRepository, swapDeductionUseCase, hydrationSwapSdkFactory)
    }
}

class HydraDxAssetExchange @AssistedInject constructor(
    @Assisted private val chain: Chain,
    @Assisted private val swapHost: AssetExchange.SwapHost,
    private val chainStateRepository: ChainStateRepository,
    private val swapDeductionUseCase: AssetInAdditionalSwapDeductionUseCase,
    private val hydrationSwapSdkFactory: HydrationSwapSdk.Factory,
) : AssetExchange {
    private val sdk by swapHost.scope.lazyAsync {
        hydrationSwapSdkFactory.create(
            coroutineScope = swapHost.scope,
            weightSpec = WeightSpec.fromBaseWeight(SwapWeights.DEFAULT_SEGMENT_WEIGHT),
            overridableData = swapHost.sharedSubscriptions.toOverridableData(),
            chain = chain
        )
    }

    override suspend fun sync(): Result<Unit> {
        return sdk().sync()
    }

    override suspend fun availableDirectSwapConnections(): List<SwapGraphEdge> {
        return sdk().availableSwapDirections().map(::DelegatingHydrationEdge)
    }

    override suspend fun feePaymentOverrides(): List<FeePaymentProviderOverride> {
        return listOf(
            FeePaymentProviderOverride(
                provider = sdk(),
                chainId = chain.id
            )
        )
    }

    override fun runSubscriptions(): Flow<ReQuoteTrigger> {
        return flowOfAll { sdk().runSubscriptions() }
    }

    private fun SharedSwapSubscriptions.toOverridableData(): HydrationSwapOverridableData {
        return object : HydrationSwapOverridableData {
            override suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber> {
                return this@toOverridableData.blockNumber(chainId)
            }
        }
    }

    private fun SwapDirection.toHydrationDirection(): HydrationSwapDirection {
        return when (this) {
            SwapDirection.SPECIFIED_IN -> HydrationSwapDirection.SPECIFIED_IN
            SwapDirection.SPECIFIED_OUT -> HydrationSwapDirection.SPECIFIED_OUT
        }
    }

    private fun SwapLimit.toHydrationSwapLimit(): HydrationSwapLimit {
        return when (this) {
            is SwapLimit.SpecifiedIn -> HydrationSwapLimit.SpecifiedIn(
                amountIn = amountIn,
                amountOutMin = amountOutMin
            )

            is SwapLimit.SpecifiedOut -> HydrationSwapLimit.SpecifiedOut(
                amountOut = amountOut,
                amountInMax = amountInMax
            )
        }
    }

    private inner class DelegatingHydrationEdge(
        val delegate: HydrationSwapEdge,
    ) : SwapGraphEdge, Edge<FullChainAssetId> by delegate {
        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): EdgeWeight {
            val delegateWeight = delegate.weightForAppendingTo(path)
            return reduceWeightWhenAppendingToHydrationSwap(path, delegateWeight)
        }

        override suspend fun quote(
            amount: Balance,
            direction: SwapDirection
        ): Balance {
            return delegate.quote(amount, direction.toHydrationDirection())
        }

        override suspend fun beginOperation(args: AtomicSwapOperationArgs): AtomicSwapOperation {
            return HydraDxOperation(delegate, args)
        }

        override suspend fun appendToOperation(
            currentTransaction: AtomicSwapOperation,
            args: AtomicSwapOperationArgs
        ): AtomicSwapOperation? {
            if (currentTransaction !is HydraDxOperation) return null

            return currentTransaction.appendSegment(delegate, args)
        }

        override suspend fun beginOperationPrototype(): AtomicSwapOperationPrototype {
            return HydraDxOperationPrototype(from.chainId)
        }

        override suspend fun appendToOperationPrototype(currentTransaction: AtomicSwapOperationPrototype): AtomicSwapOperationPrototype? {
            return currentTransaction as? HydraDxOperationPrototype
        }

        override suspend fun debugLabel(): String {
            return delegate.debugLabel()
        }

        override fun predecessorHandlesFees(predecessor: SwapGraphEdge): Boolean {
            // When chaining multiple hydra edges together, the fee is always paid with the starting edge
            return predecessor is DelegatingHydrationEdge
        }

        override suspend fun canPayNonNativeFeesInIntermediatePosition(): Boolean {
            return true
        }

        override suspend fun canTransferOutWholeAccountBalance(): Boolean {
            return true
        }

        private fun reduceWeightWhenAppendingToHydrationSwap(path: Path<WeightedEdge<FullChainAssetId>>, edgeWeight: EdgeWeight): Int {
            // Significantly reduce weight of consequent hydration segments since they are collapsed into single tx
            return if (path.isNotEmpty() && path.last() is DelegatingHydrationEdge) {
                // We divide here by 10 to achieve two goals:
                // 1. Divisor should be significant enough to allow multiple appended segments to be added without influencing total hydration weight much
                // 2. On the other hand, divisor cannot be extremely large as we will loose precision and it wont be possible
                // to distinguish different hydration segments weights between each other.
                // That is also why OMNIPOOL, STABLESWAP and XYK differ by a multiple of ten
                (edgeWeight / 10)
            } else {
                edgeWeight
            }
        }
    }

    inner class HydraDxOperationPrototype(override val fromChain: ChainId) :
        AtomicSwapOperationPrototype {
        override suspend fun roughlyEstimateNativeFee(usdConverter: UsdConverter): BigDecimal {
            // in HDX
            return 0.5.toBigDecimal()
        }

        override suspend fun maximumExecutionTime(): Duration {
            return chainStateRepository.expectedBlockTime(chain.id)
        }
    }

    inner class HydraDxOperation private constructor(
        val segments: List<HydraDxSwapTransactionSegment>,
        val feeAsset: Chain.Asset,
    ) : AtomicSwapOperation {
        override val estimatedSwapLimit: SwapLimit = aggregatedSwapLimit()

        override val assetOut: FullChainAssetId = segments.last().edge.to

        override val assetIn: FullChainAssetId = segments.first().edge.from

        override val supportsCustomRecipient: Boolean = false

        constructor(sourceEdge: HydrationSwapEdge, args: AtomicSwapOperationArgs) : this(
            listOf(HydraDxSwapTransactionSegment(sourceEdge, args.estimatedSwapLimit)),
            args.feePayment
        )

        fun appendSegment(
            nextEdge: HydrationSwapEdge,
            nextSwapArgs: AtomicSwapOperationArgs
        ): HydraDxOperation {
            val nextSegment = HydraDxSwapTransactionSegment(nextEdge, nextSwapArgs.estimatedSwapLimit)

            // Ignore nextSwapArgs.feePaymentCurrency - we are using configuration from the very first segment
            return HydraDxOperation(segments + nextSegment, feeAsset)
        }

        override suspend fun estimateFee(): Result<AtomicSwapOperationFee> {
            return sdk().estimateFee(
                trade = constructTrade(),
                swapLimit = estimatedSwapLimit.toHydrationSwapLimit(),
                feeAsset = feeAsset
            ).map {
                SubmissionOnlyAtomicSwapOperationFee(it)
            }
        }

        override suspend fun requiredAmountInToGetAmountOut(extraOutAmount: Balance): Balance {
            val assetInId = assetIn.assetId
            val assetIn = chain.assetsById.getValue(assetInId)

            val assetOutId = assetOut.assetId
            val assetOut = chain.assetsById.getValue(assetOutId)

            val quoteArgs = ParentQuoterArgs(
                chainAssetIn = assetIn,
                chainAssetOut = assetOut,
                amount = extraOutAmount,
                swapDirection = SwapDirection.SPECIFIED_OUT
            )

            return swapHost.quote(quoteArgs)
        }

        private fun constructTrade(): Path<HydrationSwapEdge> {
            return segments.map { it.edge }
        }

        override suspend fun additionalMaxAmountDeduction(metaAccount: MetaAccount): SwapMaxAdditionalAmountDeduction {
            val assetInId = assetIn.assetId
            val assetIn = chain.assetsById.getValue(assetInId)

            val assetOutId = assetOut.assetId
            val assetOut = chain.assetsById.getValue(assetOutId)

            return SwapMaxAdditionalAmountDeduction(
                fromCountedTowardsEd = swapDeductionUseCase.invoke(assetIn, assetOut, metaAccount)
            )
        }

        override suspend fun execute(args: AtomicSwapOperationSubmissionArgs): Result<SwapExecutionOutcome> {
            return sdk().submit(
                trade = constructTrade(),
                swapLimit = args.actualSwapLimit.toHydrationSwapLimit(),
                feeAsset = feeAsset,
                origin = args.origin
            )
                .map {
                    SwapExecutionOutcome(
                        actualReceivedAmount = it.actualReceivedAmount,
                    )
                }
        }

        private fun aggregatedSwapLimit(): SwapLimit {
            val firstLimit = segments.first().swapLimit
            val lastLimit = segments.last().swapLimit

            return SwapLimit.createAggregated(firstLimit, lastLimit)
        }
    }

    class HydraDxSwapTransactionSegment(
        val edge: HydrationSwapEdge,
        val swapLimit: SwapLimit,
    )
}
