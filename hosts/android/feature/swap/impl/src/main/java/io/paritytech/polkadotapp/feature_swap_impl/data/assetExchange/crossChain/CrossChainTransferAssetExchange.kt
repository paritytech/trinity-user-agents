package io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.crossChain

import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.getChainOrNull
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.emptyAccountId
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.graph.Edge
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.SimpleEdge
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.CrossChainTransferService
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.AmountAdjustmentMode
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransfer
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDirectionId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDryRunOrigin
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferDryRunOutcome
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFeatures
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransferFee
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.canPayCrossChainFeesFromTransferringAmount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.canTransferOutWholeBalance
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.originChain
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AccountFeeWithLabel
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperation
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationPrototype
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationSubmissionArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.FeeWithLabel
import io.paritytech.polkadotapp.feature_swap_api.domain.model.ReQuoteTrigger
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapExecutionOutcome
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapLimit
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapMaxAdditionalAmountDeduction
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapOperationSubmissionException
import io.paritytech.polkadotapp.feature_swap_api.domain.model.UsdConverter
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.AssetExchange
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.FeePaymentProviderOverride
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.SwapWeights
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.getFeePaymentOrNative
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.metaAccountOrThrow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Duration

class CrossChainTransferAssetExchangeFactory @Inject constructor(
    private val service: CrossChainTransferService,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains
) : AssetExchange.MultiChainFactory {
    override suspend fun create(swapHost: AssetExchange.SwapHost): AssetExchange {
        return CrossChainTransferAssetExchange(swapHost, service, chainRegistry, knownChains)
    }
}

class CrossChainTransferAssetExchange @AssistedInject constructor(
    @Assisted private val swapHost: AssetExchange.SwapHost,
    private val service: CrossChainTransferService,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains
) : AssetExchange, ComputationalScope by swapHost.scope {
    override suspend fun sync(): Result<Unit> {
        return Result.success(Unit)
    }

    override suspend fun availableDirectSwapConnections(): List<SwapGraphEdge> {
        return service.availableDirectionIds().map(::CrossChainTransferEdge)
    }

    override suspend fun feePaymentOverrides(): List<FeePaymentProviderOverride> {
        return emptyList()
    }

    override fun runSubscriptions(): Flow<ReQuoteTrigger> {
        return emptyFlow()
    }

    inner class CrossChainTransferEdge(
        val delegate: CrossChainTransferDirectionId
    ) : SwapGraphEdge, Edge<FullChainAssetId> by delegate {
        private var transferFeatures: CrossChainTransferFeatures? = null

        override suspend fun beginOperation(args: AtomicSwapOperationArgs): AtomicSwapOperation {
            return CrossChainTransferOperation(args, this)
        }

        override suspend fun appendToOperation(currentTransaction: AtomicSwapOperation, args: AtomicSwapOperationArgs): AtomicSwapOperation? {
            return null
        }

        override suspend fun beginOperationPrototype(): AtomicSwapOperationPrototype {
            return CrossChainTransferOperationPrototype(this)
        }

        override suspend fun appendToOperationPrototype(currentTransaction: AtomicSwapOperationPrototype): AtomicSwapOperationPrototype? {
            return null
        }

        override suspend fun debugLabel(): String? {
            val name = chainRegistry.getChainOrNull(delegate.to.chainId)?.name ?: return null
            return "To $name"
        }

        override fun predecessorHandlesFees(predecessor: SwapGraphEdge): Boolean {
            return false
        }

        override suspend fun canPayNonNativeFeesInIntermediatePosition(): Boolean {
            return getTransferFeatures().canPayCrossChainFeesFromTransferringAmount()
        }

        override suspend fun canTransferOutWholeAccountBalance(): Boolean {
            return getTransferFeatures().canTransferOutWholeBalance()
        }

        override suspend fun quote(amount: Balance, direction: SwapDirection): Balance {
            return amount
        }

        override fun weightForAppendingTo(path: Path<WeightedEdge<FullChainAssetId>>): Int {
            return SwapWeights.CrossChainTransfer.TRANSFER
        }

        private suspend fun getTransferFeatures(): CrossChainTransferFeatures {
            if (transferFeatures == null) {
                transferFeatures = service.getTransferFeatures(delegate)
            }

            return transferFeatures!!
        }
    }

    inner class CrossChainTransferOperationPrototype(
        private val directionId: CrossChainTransferDirectionId,
    ) : AtomicSwapOperationPrototype {
        override val fromChain: ChainId = directionId.from.chainId

        private val toChain: ChainId = directionId.to.chainId

        override suspend fun roughlyEstimateNativeFee(usdConverter: UsdConverter): BigDecimal {
            var totalAmount = BigDecimal.ZERO

            if (isChainWithExpensiveCrossChain(fromChain)) {
                totalAmount += usdConverter.nativeAssetEquivalentOf(0.15)
            }

            if (isChainWithExpensiveCrossChain(toChain)) {
                totalAmount += usdConverter.nativeAssetEquivalentOf(0.1)
            }

            if (!(isChainWithExpensiveCrossChain(fromChain) || isChainWithExpensiveCrossChain(toChain))) {
                totalAmount += usdConverter.nativeAssetEquivalentOf(0.01)
            }

            return totalAmount
        }

        override suspend fun maximumExecutionTime(): Duration {
            return service.estimateMaximumExecutionTime(directionId)
        }

        private fun isChainWithExpensiveCrossChain(chainId: ChainId): Boolean {
            return chainId == knownChains.assetHub || chainId == knownChains.people
        }
    }

    inner class CrossChainTransferOperation(
        private val transactionArgs: AtomicSwapOperationArgs,
        private val edge: Edge<FullChainAssetId>
    ) : AtomicSwapOperation {
        override val estimatedSwapLimit: SwapLimit = transactionArgs.estimatedSwapLimit

        override val assetOut: FullChainAssetId = edge.to

        override val assetIn: FullChainAssetId = edge.from

        override val supportsCustomRecipient: Boolean = true

        override suspend fun estimateFee(): Result<AtomicSwapOperationFee> {
            val transfer = createTransfer(
                amount = estimatedSwapLimit.crossChainTransferAmount,
                recipient = chainRegistry.getChain(edge.from.chainId).emptyAccountId()
            )

            return service.estimateFee(transfer).map(::CrossChainAtomicOperationFee)
        }

        override suspend fun requiredAmountInToGetAmountOut(extraOutAmount: Balance): Balance {
            return extraOutAmount
        }

        override suspend fun additionalMaxAmountDeduction(metaAccount: MetaAccount): SwapMaxAdditionalAmountDeduction {
            return SwapMaxAdditionalAmountDeduction(
                fromCountedTowardsEd = service.requiredRemainingAmountAfterTransfer(edge)
            )
        }

        override suspend fun execute(args: AtomicSwapOperationSubmissionArgs): Result<SwapExecutionOutcome> {
            val transfer = createTransfer(
                amount = args.actualSwapLimit.crossChainTransferAmount,
                recipient = args.recipient
            )
            val sender = args.origin.metaAccountOrThrow()

            return dryRunTransfer(transfer, sender)
                .flatMap { service.performAndTrackTransfer(transfer, sender) }
                .map {
                    SwapExecutionOutcome(
                        actualReceivedAmount = it.receivedOnDestination,
                    )
                }
        }

        private suspend fun dryRunTransfer(
            transfer: CrossChainTransfer,
            sender: MetaAccount
        ): Result<CrossChainTransferDryRunOutcome> {
            val origin = sender.accountIdIn(transfer.originChain)

            return service.dryRunTransfer(
                transfer = transfer,
                dryRunOrigin = CrossChainTransferDryRunOrigin.Signed(origin)
            ).mapError { SwapOperationSubmissionException.SimulationFailed() }
        }

        private suspend fun createTransfer(
            amount: Balance,
            recipient: AccountId
        ): CrossChainTransfer {
            val origin = chainRegistry.chainWithAsset(edge.from)
            val destination = chainRegistry.chainWithAsset(edge.to)

            return CrossChainTransfer(
                recipient = recipient,
                direction = SimpleEdge(origin, destination),
                userSpecifiedAmount = amount,
                amountAdjustmentMode = AmountAdjustmentMode.Exact,
                originFeePayment = swapHost.getFeePaymentProvider().getFeePaymentOrNative(transactionArgs.feePayment),
            )
        }

        private val SwapLimit.crossChainTransferAmount: Balance
            get() = when (this) {
                is SwapLimit.SpecifiedIn -> amountIn
                is SwapLimit.SpecifiedOut -> amountOut
            }
    }

    private class CrossChainAtomicOperationFee(
        crossChainFee: CrossChainTransferFee
    ) : AtomicSwapOperationFee {
        override val submissionFee = AccountFeeWithLabel(crossChainFee.submissionFee)

        override val postSubmissionFees = AtomicSwapOperationFee.PostSubmissionFees(
            paidByAccount = listOfNotNull(
                AccountFeeWithLabel(crossChainFee.postSubmissionByAccount, debugLabel = "Delivery"),
            ),
            paidFromAmount = listOf(
                FeeWithLabel(crossChainFee.postSubmissionFromAmount, debugLabel = "Execution")
            )
        )
    }
}
