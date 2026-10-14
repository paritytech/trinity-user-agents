package io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.assetConversion

import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.graph.Edge
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
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.AssetExchange
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.FeePaymentProviderOverride
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.ParentQuoterArgs
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.SharedSwapSubscriptions
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.SwapWeights
import io.paritytech.polkadotapp.feature_swap_impl.domain.AssetInAdditionalSwapDeductionUseCase
import io.paritytech.polkadotapp.feature_swap_impl.domain.fee.SubmissionOnlyAtomicSwapOperationFee
import io.paritytech.polkadotapp.tools_assethub_sdk_api.AssetHubSdk
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSdkOverridableData
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSwapEdge
import kotlinx.coroutines.flow.Flow
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Duration
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapDirection as AssetHubSwapDirection
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapLimit as AssetHubSwapLimit

class AssetConversionExchangeFactory @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
    private val deductionUseCase: AssetInAdditionalSwapDeductionUseCase,
    private val assetHubSdkFactory: AssetHubSdk.Factory
) : AssetExchange.SingleChainFactory {
    override suspend fun create(chain: Chain, swapHost: AssetExchange.SwapHost): AssetExchange {
        return AssetConversionExchange(chain, swapHost, chainStateRepository, deductionUseCase, assetHubSdkFactory)
    }
}

class AssetConversionExchange @AssistedInject constructor(
    @Assisted private val chain: Chain,
    @Assisted private val swapHost: AssetExchange.SwapHost,
    private val chainStateRepository: ChainStateRepository,
    private val deductionUseCase: AssetInAdditionalSwapDeductionUseCase,
    private val assetHubSdkFactory: AssetHubSdk.Factory,
) : AssetExchange {
    private val sdk by swapHost.scope.lazyAsync {
        assetHubSdkFactory.create(
            chainId = chain.id,
            overridableData = swapHost.sharedSubscriptions.toOverridableData()
        )
    }

    override suspend fun sync(): Result<Unit> {
        return Result.success(Unit)
    }

    override suspend fun availableDirectSwapConnections(): List<SwapGraphEdge> {
        return sdk().availableSwapDirections().map(::AssetConversionEdge)
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
        return flowOfAll { sdk().quoteInvalidationFlow() }
    }

    private fun SharedSwapSubscriptions.toOverridableData(): AssetHubSdkOverridableData {
        return object : AssetHubSdkOverridableData {
            override suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber> {
                return this@toOverridableData.blockNumber(chainId)
            }
        }
    }

    private fun SwapDirection.toAssetHubDirection(): AssetHubSwapDirection {
        return when (this) {
            SwapDirection.SPECIFIED_IN -> AssetHubSwapDirection.SPECIFIED_IN
            SwapDirection.SPECIFIED_OUT -> AssetHubSwapDirection.SPECIFIED_OUT
        }
    }

    private fun SwapLimit.toAssetHubSwapLimit(): AssetHubSwapLimit {
        return when (this) {
            is SwapLimit.SpecifiedIn -> AssetHubSwapLimit.SpecifiedIn(
                amountIn = amountIn,
                amountOutMin = amountOutMin
            )

            is SwapLimit.SpecifiedOut -> AssetHubSwapLimit.SpecifiedOut(
                amountOut = amountOut,
                amountInMax = amountInMax
            )
        }
    }

    private inner class AssetConversionEdge(
        private val delegate: AssetHubSwapEdge
    ) : Edge<FullChainAssetId> by delegate, SwapGraphEdge {
        override suspend fun beginOperation(args: AtomicSwapOperationArgs): AtomicSwapOperation {
            return AssetConversionOperation(args, delegate)
        }

        override suspend fun appendToOperation(
            currentTransaction: AtomicSwapOperation,
            args: AtomicSwapOperationArgs
        ): AtomicSwapOperation? {
            return null
        }

        override suspend fun beginOperationPrototype(): AtomicSwapOperationPrototype {
            return AssetConversionOperationPrototype(delegate.fromAsset.chainId)
        }

        override suspend fun appendToOperationPrototype(currentTransaction: AtomicSwapOperationPrototype): AtomicSwapOperationPrototype? {
            return null
        }

        override suspend fun debugLabel(): String {
            return "AssetConversion"
        }

        override fun predecessorHandlesFees(predecessor: SwapGraphEdge): Boolean {
            return false
        }

        override suspend fun canPayNonNativeFeesInIntermediatePosition(): Boolean {
            return true
        }

        override suspend fun canTransferOutWholeAccountBalance(): Boolean {
            return true
        }

        override suspend fun quote(
            amount: Balance,
            direction: SwapDirection
        ): Balance {
            return delegate.quote(amount, direction.toAssetHubDirection())
        }

        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): Int {
            return SwapWeights.AssetConversion.SWAP
        }
    }

    inner class AssetConversionOperationPrototype(override val fromChain: ChainId) :
        AtomicSwapOperationPrototype {
        override suspend fun roughlyEstimateNativeFee(usdConverter: UsdConverter): BigDecimal {
            // in DOT
            return 0.0015.toBigDecimal()
        }

        override suspend fun maximumExecutionTime(): Duration {
            return chainStateRepository.expectedBlockTime(chain.id)
        }
    }

    inner class AssetConversionOperation(
        private val transactionArgs: AtomicSwapOperationArgs,
        private val edge: AssetHubSwapEdge,
    ) : AtomicSwapOperation {
        private val fromAsset: Chain.Asset = edge.fromAsset
        private val toAsset: Chain.Asset = edge.toAsset

        override val estimatedSwapLimit: SwapLimit = transactionArgs.estimatedSwapLimit

        override val assetOut: FullChainAssetId = toAsset.fullId

        override val assetIn: FullChainAssetId = fromAsset.fullId

        override val supportsCustomRecipient: Boolean = true

        override suspend fun estimateFee(): Result<AtomicSwapOperationFee> {
            return sdk().estimateFee(
                trade = edge,
                swapLimit = estimatedSwapLimit.toAssetHubSwapLimit(),
                feeAsset = transactionArgs.feePayment
            )
                .map { SubmissionOnlyAtomicSwapOperationFee(it) }
        }

        override suspend fun requiredAmountInToGetAmountOut(extraOutAmount: Balance): Balance {
            val quoteArgs = ParentQuoterArgs(
                chainAssetIn = fromAsset,
                chainAssetOut = toAsset,
                amount = extraOutAmount,
                swapDirection = SwapDirection.SPECIFIED_OUT
            )

            return swapHost.quote(quoteArgs)
        }

        override suspend fun additionalMaxAmountDeduction(metaAccount: MetaAccount): SwapMaxAdditionalAmountDeduction {
            return SwapMaxAdditionalAmountDeduction(
                fromCountedTowardsEd = deductionUseCase.invoke(fromAsset, toAsset, metaAccount)
            )
        }

        override suspend fun execute(args: AtomicSwapOperationSubmissionArgs): Result<SwapExecutionOutcome> {
            return sdk().performSwap(
                trade = edge,
                swapLimit = args.actualSwapLimit.toAssetHubSwapLimit(),
                feeAsset = transactionArgs.feePayment,
                origin = args.origin,
                recipient = args.recipient
            )
                .map {
                    SwapExecutionOutcome(
                        actualReceivedAmount = it.actualReceivedAmount,
                    )
                }
        }
    }
}
