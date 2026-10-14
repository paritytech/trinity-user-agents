package io.paritytech.polkadotapp.feature_swap_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.ChainsById
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.asset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.chains.multiNetwork.chainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.chainWithAssetOrNull
import io.paritytech.polkadotapp.chains.multiNetwork.chainsById
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.orZero
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.chains.util.isUtility
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.chains.util.utilityAsset
import io.paritytech.polkadotapp.chains.util.utilityAssetId
import io.paritytech.polkadotapp.common.data.memory.ComputationalCache
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.memory.MapCache
import io.paritytech.polkadotapp.common.data.memory.SharedFlowMapCache
import io.paritytech.polkadotapp.common.data.memory.useCache
import io.paritytech.polkadotapp.common.data.memory.useSharedFlow
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.asFraction
import io.paritytech.polkadotapp.common.utils.atLeastZero
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatten
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.forEachAsync
import io.paritytech.polkadotapp.common.utils.graph.EdgeVisitFilter
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.allEdges
import io.paritytech.polkadotapp.common.utils.graph.create
import io.paritytech.polkadotapp.common.utils.graph.findAllPossibleDestinations
import io.paritytech.polkadotapp.common.utils.graph.hasOutcomingDirections
import io.paritytech.polkadotapp.common.utils.graph.numberOfEdges
import io.paritytech.polkadotapp.common.utils.graph.vertices
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.invoke
import io.paritytech.polkadotapp.common.utils.isZero
import io.paritytech.polkadotapp.common.utils.lazyAsync
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.measureExecution
import io.paritytech.polkadotapp.common.utils.mergeIfMultiple
import io.paritytech.polkadotapp.common.utils.orZero
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.common.utils.withFlowScope
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_prices_api.domain.GetCachedPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.GetPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.model.PriceLookup
import io.paritytech.polkadotapp.feature_prices_api.domain.model.amountOf
import io.paritytech.polkadotapp.feature_prices_api.domain.model.priceOf
import io.paritytech.polkadotapp.feature_swap_api.domain.SwapService
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperation
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationPrototype
import io.paritytech.polkadotapp.feature_swap_api.domain.model.AtomicSwapOperationSubmissionArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.PathRoughFeeEstimation
import io.paritytech.polkadotapp.feature_swap_api.domain.model.QuotedEdge
import io.paritytech.polkadotapp.feature_swap_api.domain.model.QuotedPath
import io.paritytech.polkadotapp.feature_swap_api.domain.model.ReQuoteTrigger
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapExecutionEstimate
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapExecutionOutcome
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapFee
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapFeeArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraph
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapLimit
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapProgress
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapProgressStep
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapQuote
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapQuoteArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.UsdConverter
import io.paritytech.polkadotapp.feature_swap_api.domain.model.WeightBreakdown
import io.paritytech.polkadotapp.feature_swap_api.domain.model.WithDepositedAmount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.amountToLeaveOnOriginToPayTxFees
import io.paritytech.polkadotapp.feature_swap_api.domain.model.estimatedAmountIn
import io.paritytech.polkadotapp.feature_swap_api.domain.model.estimatedAmountOut
import io.paritytech.polkadotapp.feature_swap_api.domain.model.firstSegmentQuote
import io.paritytech.polkadotapp.feature_swap_api.domain.model.firstSegmentQuotedAmount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.lastSegmentQuote
import io.paritytech.polkadotapp.feature_swap_api.domain.model.lastSegmentQuotedAmount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.replaceAmountIn
import io.paritytech.polkadotapp.feature_swap_api.domain.model.swapNeedsToRecipientTransfer
import io.paritytech.polkadotapp.feature_swap_api.domain.model.totalFeeEnsuringSubmissionAsset
import io.paritytech.polkadotapp.feature_swap_impl.BuildConfig
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.AssetExchange
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.ParentQuoterArgs
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.SharedSwapSubscriptions
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.assetConversion.AssetConversionExchangeFactory
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.crossChain.CrossChainTransferAssetExchangeFactory
import io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange.hydraDx.HydraDxAssetExchangeFactory
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.GraphDebugger
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.PathFeeEstimator
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.PathQuoter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FastLookupCustomFeeCapability
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePaymentProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePaymentRegistry
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleFee
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.getFeePaymentOrNative
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import io.paritytech.polkadotapp.feature_transfers_api.data.type.TokenTransfersTypeRegistry
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.TransferArguments
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.withIndex
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.math.BigDecimal
import java.math.MathContext
import javax.inject.Inject
import kotlin.collections.fold
import kotlin.collections.withIndex
import kotlin.map
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

private const val ALL_DIRECTIONS_CACHE = "RealSwapService.ALL_DIRECTIONS"
private const val EXCHANGES_CACHE = "RealSwapService.EXCHANGES"
private const val QUOTER_CACHE = "RealSwapService.QUOTER"
private const val NODE_VISIT_FILTER = "RealSwapService.NodeVisitFilter"
private const val SHARED_SUBSCRIPTIONS = "RealSwapService.SharedSubscriptions"

private val ADDITIONAL_ESTIMATE_BUFFER = 3.seconds

internal class RealSwapService @Inject constructor(
    private val assetConversionFactory: AssetConversionExchangeFactory,
    private val hydraDxExchangeFactory: HydraDxAssetExchangeFactory,
    private val crossChainTransferFactory: CrossChainTransferAssetExchangeFactory,
    private val computationalCache: ComputationalCache,
    private val chainRegistry: ChainRegistry,
    private val quoterFactory: PathQuoter.Factory,
    private val assetSourceRegistry: TokenBalanceTypeRegistry,
    private val getCachedPriceUse: GetCachedPriceUseCase,
    private val getPriceUse: GetPriceUseCase,
    private val extrinsicService: ExtrinsicService,
    private val chainStateRepository: ChainStateRepository,
    private val knownChains: KnownChains,
    private val graphDebugger: GraphDebugger,
    private val signedOrigins: SignedOrigins,
    // For debug purposes only
    private val amountFormatter: TokenAmountFormatter,
    private val tokenAmountMapper: TokenAmountMapper,
    private val tokenTransfersTypeRegistry: TokenTransfersTypeRegistry,
    private val coroutineDispatchers: CoroutineDispatchers,
) : SwapService {
    private val debug: Boolean = BuildConfig.DEBUG

    context(scope: ComputationalScope)
    override fun initiateWarmUp() {
        scope.launch(coroutineDispatchers.io) {
            // Launch warm up parts concurrently
            launch { warmUpChain(knownChains.assetHub) }

            knownChains.hydration?.let {
                launch { warmUpChain(it) }
            }

            launch { directionsGraphState().first() }
        }
    }

    context(scope: ComputationalScope)
    private suspend fun warmUpChain(chainId: ChainId) {
        nodeVisitFilter().warmUpChain(chainId)
    }

    context(scope: ComputationalScope)
    override suspend fun sync() {
        exchangeRegistry()
            .allExchanges()
            .forEachAsync {
                it.sync()
                    .logFailure("Failed to sync ${it::class.simpleName}")
            }
    }

    context(scope: ComputationalScope)
    override suspend fun assetsAvailableForSwap(): Flow<Set<FullChainAssetId>> {
        return directionsGraph().map { it.vertices() }
    }

    context(scope: ComputationalScope)
    override suspend fun awaitFullyLoadedRouting() {
        directionsGraphState().first { it.fullyLoaded }
    }

    context(scope: ComputationalScope)
    override suspend fun availableSwapDirectionsFor(
        asset: Chain.Asset,
    ): Flow<Set<FullChainAssetId>> {
        return directionsGraph().map {
            val filter = nodeVisitFilter()
            measureExecution("findAllPossibleDestinations") {
                it.findAllPossibleDestinations(asset.fullId, filter) - asset.fullId
            }
        }
    }

    context(scope: ComputationalScope)
    override suspend fun hasAvailableSwapDirections(asset: Chain.Asset): Flow<Boolean> {
        return directionsGraph().map { it.hasOutcomingDirections(asset.fullId) }
    }

    context(scope: ComputationalScope)
    override suspend fun quote(
        args: SwapQuoteArgs,
    ): Result<SwapQuote> {
        return withContext(Dispatchers.Default) {
            runCatching { quoteInternal(args) }
                .logFailure("Error while quoting")
        }
    }

    context(scope: ComputationalScope)
    override suspend fun estimateFee(feeArgs: SwapFeeArgs): Result<SwapFee> {
        return coroutineScope {
            val atomicOperations = feeArgs.constructAtomicOperations()

            val swapFees = async {
                atomicOperations.mapAsync { operation -> operation.estimateFee()
                    .map { SwapFee.SwapSegment(it, operation) }
                    .logFailure("Failed to load fee for ${operation::class.simpleName}")
                }
                    .flatten()
            }

            val finalTransferFee = async {
                calculateFinalTransferFee(
                    finalSwapOperation = atomicOperations.last(),
                    recipient = feeArgs.recipient,
                    sender = feeArgs.sender
                )
            }

            swapFees().flatMap { swapFees ->
                finalTransferFee().mapCatching { transferFee ->
                    val convertedFees = convertIntermediateSegmentsFeesToAssetIn(feeArgs.assetIn, swapFees, transferFee)
                    val firstOperation = atomicOperations.first()

                    SwapFee(
                        segments = swapFees,
                        recipientTransferFee = transferFee,
                        intermediateSegmentFeesInAssetIn = convertedFees,
                        additionalMaxAmountDeduction = firstOperation.additionalMaxAmountDeduction(feeArgs.sender),
                        sender = feeArgs.sender,
                        recipient = feeArgs.recipient
                    ).also(::logFee)
                }
            }
        }
    }

    context(scope: ComputationalScope)
    override suspend fun swap(calculatedFee: SwapFee): Flow<SwapProgress> {
        val segments = calculatedFee.segments

        val initialCorrection: Result<SwapExecutionOutcome?> = Result.success(null)

        return flow {
            segments.withIndex().fold(initialCorrection) { prevStepCorrection, (index, segment) ->
                val operation = segment.operation

                prevStepCorrection.flatMap { correction ->
                    val step = SwapProgressStep(index, operation)
                    emit(SwapProgress.SegmentStarted(step))

                    val segmentRecipient = determineSegmentRecipient(index, calculatedFee, calculatedFee.recipient)
                    val newArgs = constructOperationArgs(correction, segment, calculatedFee, segmentRecipient)

                    val debugRepresentation = operation.debugRepresentation(newArgs.actualSwapLimit)
                    Timber.d("$debugRepresentation with ${newArgs.actualSwapLimit}")

                    operation.execute(newArgs).onFailure {
                        Timber.e(it, "Swap failed on stage '$debugRepresentation'")

                        emit(SwapProgress.SegmentFailure(it, attemptedStep = step))
                    }
                }
            }
                .requireNotNull()
                .flatMap { outcome ->
                    if (calculatedFee.swapNeedsToRecipientTransfer()) {
                        emit(SwapProgress.ToRecipientTransferStarted)
                        performTransferToRecipient(outcome, calculatedFee)
                            .logFailure("Failed to perform a to recipient transfer")
                            .onFailure { emit(SwapProgress.TransferFailure(it)) }
                    } else {
                        Result.success(outcome)
                    }
                }.onSuccess {
                    emit(SwapProgress.Done(it.actualReceivedAmount))
                }
        }
    }

    context(scope: ComputationalScope)
    private suspend fun performTransferToRecipient(
        swapExecutionOutcome: SwapExecutionOutcome,
        swapFee: SwapFee
    ): Result<SwapExecutionOutcome> {
        val transferFee = swapFee.recipientTransferFee!!
        val feePayment = exchangeRegistry().feePaymentProvider().getFeePaymentOrNative(transferFee.asset)
        val transferArgs = TransferArguments(
            recipient = swapFee.recipient,
            amount = swapExecutionOutcome.actualReceivedAmount - transferFee.amount,
            feePayment = feePayment,
            origin = signedOrigins.signedOrigin(swapFee.sender)
        )

        return tokenTransfersTypeRegistry.typeFor(transferFee.asset)
            .performAndTrackTransfer(transferArgs)
            .flattenExecutionFailure()
            .map { SwapExecutionOutcome(actualReceivedAmount = transferArgs.amount) }
    }

    context(scope: ComputationalScope)
    private suspend fun calculateFinalTransferFee(
        finalSwapOperation: AtomicSwapOperation,
        recipient: AccountId,
        sender: MetaAccount,
    ): Result<AccountFee?> {
        if (finalSwapOperation.supportsCustomRecipient) return Result.success(null)

        val (chain, asset) = chainRegistry.chainWithAsset(finalSwapOperation.assetOut)
        if (sender.accountIdIn(chain) == recipient) return Result.success(null)

        val transfer = TransferArguments(
            recipient = recipient,
            amount = finalSwapOperation.estimatedSwapLimit.estimatedAmountOut,
            feePayment = exchangeRegistry().feePaymentProvider().getFeePaymentOrNative(asset),
            origin = signedOrigins.signedOrigin(sender)
        )

        return tokenTransfersTypeRegistry.typeFor(asset)
            .calculateFee(transfer)
    }

    private suspend fun determineSegmentRecipient(
        operationIndex: Int,
        swapFee: SwapFee,
        finalRecipient: AccountId
    ): AccountId {
        val isFinalOperation = operationIndex == swapFee.segments.size - 1
        return if (isFinalOperation) {
            finalRecipient
        } else {
            val segment = swapFee.segments[operationIndex]
            val chain = chainRegistry.getChain(segment.operation.assetOut.chainId)

            swapFee.sender.accountIdIn(chain)
        }
    }

    private suspend fun constructOperationArgs(
        previousOutcome: WithDepositedAmount?,
        segment: SwapFee.SwapSegment,
        swapFee: SwapFee,
        recipient: AccountId
    ): AtomicSwapOperationSubmissionArgs {
        val actualSwapLimit = adjustSwapLimit(previousOutcome, segment, swapFee)

        return AtomicSwapOperationSubmissionArgs(
            actualSwapLimit = actualSwapLimit,
            origin = signedOrigins.signedOrigin(swapFee.sender),
            recipient = recipient
        )
    }

    private fun adjustSwapLimit(
        previousOutcome: WithDepositedAmount?,
        segment: SwapFee.SwapSegment,
        swapFee: SwapFee
    ): SwapLimit {
        val newAmountIn = determineNewAmountIn(previousOutcome, segment, swapFee)

        // We cannot execute buy for segments after first one since we deal with actualReceivedAmount there
        val shouldReplaceBuyWithSell = previousOutcome != null
        val actualSwapLimit = segment.operation.estimatedSwapLimit.replaceAmountIn(
            newAmountIn,
            shouldReplaceBuyWithSell
        )

        return actualSwapLimit
    }

    private fun determineNewAmountIn(
        previousOutcome: WithDepositedAmount?,
        segment: SwapFee.SwapSegment,
        swapFee: SwapFee,
    ): Balance {
        return if (previousOutcome != null) {
            previousOutcome.actualReceivedAmount - segment.fee.amountToLeaveOnOriginToPayTxFees()
        } else {
            val amountIn = segment.operation.estimatedSwapLimit.estimatedAmountIn()
            amountIn + swapFee.additionalAmountForSwap()
        }
    }

    private suspend fun AtomicSwapOperation.debugRepresentation(actualSwapLimit: SwapLimit): String {
        val from = chainRegistry.asset(assetIn)
        val to = chainRegistry.asset(assetOut)
        return "${actualSwapLimit.estimatedAmountIn} ${from.symbol} -> ${actualSwapLimit.estimatedAmountOut} ${to.symbol} (${this::class.simpleName})"
    }

    private fun SwapLimit.estimatedAmountIn(): Balance {
        return when (this) {
            is SwapLimit.SpecifiedIn -> amountIn
            is SwapLimit.SpecifiedOut -> amountInQuote
        }
    }

    private suspend fun convertIntermediateSegmentsFeesToAssetIn(
        assetIn: Chain.Asset,
        segments: List<SwapFee.SwapSegment>,
        toRecipientFee: AccountFee?,
    ): Fee {
        val transferFeeInAssetOut = toRecipientFee?.amount.orZero()

        val convertedFees = segments.foldRightIndexed(transferFeeInAssetOut) { index, (operationFee, swapOperation), futureFeePlanks ->
            val amountInToGetFeesForOut = if (futureFeePlanks.isPositive()) {
                swapOperation.requiredAmountInToGetAmountOut(futureFeePlanks)
            } else {
                Balance.ZERO
            }

            amountInToGetFeesForOut + if (index != 0) {
                // Ensure everything is in the same asset
                operationFee.totalFeeEnsuringSubmissionAsset()
            } else {
                // First segment is not included
                Balance.ZERO
            }
        }

        return SimpleFee(convertedFees, assetIn)
    }

    private suspend fun SwapFeeArgs.constructAtomicOperations(): List<AtomicSwapOperation> {
        var currentSwapTx: AtomicSwapOperation? = null
        val finishedSwapTxs = mutableListOf<AtomicSwapOperation>()

        executionPath.forEachIndexed { index, segmentExecuteArgs ->
            val quotedEdge = segmentExecuteArgs.quotedSwapEdge

            val operationArgs = AtomicSwapOperationArgs(
                SwapLimit(direction, quotedEdge.quotedAmount, slippage, quotedEdge.quote),
                segmentExecuteArgs.quotedSwapEdge.edge.identifySegmentFeeAsset(
                    isFirstSegment = index == 0,
                    firstSegmentFees = firstSegmentFees,
                )
            )

            // Initial case - begin first operation
            if (currentSwapTx == null) {
                currentSwapTx = quotedEdge.edge.beginOperation(operationArgs)
                return@forEachIndexed
            }

            // Try to append segment to current swap tx
            val maybeAppendedCurrentTx = quotedEdge.edge.appendToOperation(currentSwapTx!!, operationArgs)

            currentSwapTx = if (maybeAppendedCurrentTx == null) {
                finishedSwapTxs.add(currentSwapTx!!)
                quotedEdge.edge.beginOperation(operationArgs)
            } else {
                maybeAppendedCurrentTx
            }
        }

        finishedSwapTxs.add(currentSwapTx!!)

        return finishedSwapTxs
    }

    private suspend fun Path<QuotedEdge<SwapGraphEdge>>.constructAtomicOperationPrototypes(): List<AtomicSwapOperationPrototype> {
        var currentSwapTx: AtomicSwapOperationPrototype? = null
        val finishedSwapTxs = mutableListOf<AtomicSwapOperationPrototype>()

        forEach { quotedEdge ->
            // Initial case - begin first operation
            if (currentSwapTx == null) {
                currentSwapTx = quotedEdge.edge.beginOperationPrototype()
                return@forEach
            }

            // Try to append segment to current swap tx
            val maybeAppendedCurrentTx = quotedEdge.edge.appendToOperationPrototype(currentSwapTx!!)

            currentSwapTx = if (maybeAppendedCurrentTx == null) {
                finishedSwapTxs.add(currentSwapTx!!)
                quotedEdge.edge.beginOperationPrototype()
            } else {
                maybeAppendedCurrentTx
            }
        }

        finishedSwapTxs.add(currentSwapTx!!)

        return finishedSwapTxs
    }

    private suspend fun SwapGraphEdge.identifySegmentFeeAsset(
        isFirstSegment: Boolean,
        firstSegmentFees: Chain.Asset
    ): Chain.Asset {
        return if (isFirstSegment) {
            firstSegmentFees
        } else {
            // When executing intermediate segments, always pay in sending asset
            chainRegistry.asset(from)
        }
    }

    context(scope: ComputationalScope)
    private suspend fun quoteInternal(
        args: SwapQuoteArgs,
    ): SwapQuote {
        val quotedTrade = quoteTrade(
            chainAssetIn = args.assetIn,
            chainAssetOut = args.assetOut,
            amount = args.amount,
            swapDirection = args.swapDirection,
        )

        val amountIn = quotedTrade.amountIn()
        val amountOut = quotedTrade.amountOut()

        val atomicOperationsEstimates = quotedTrade.estimateOperationsMaximumExecutionTime()

        return SwapQuote(
            amountIn = args.assetIn.withAmount(amountIn),
            amountOut = args.assetOut.withAmount(amountOut),
            priceImpact = args.calculatePriceImpact(amountIn, amountOut),
            quotedPath = quotedTrade,
            executionEstimate = SwapExecutionEstimate(
                atomicOperationsEstimates,
                ADDITIONAL_ESTIMATE_BUFFER
            ),
            direction = args.swapDirection,
        )
    }

    private suspend fun QuotedTrade.estimateOperationsMaximumExecutionTime(): List<Duration> {
        return path.constructAtomicOperationPrototypes()
            .map { it.maximumExecutionTime() }
    }

    context(computationalScope: ComputationalScope)
    override fun runSubscriptions(): Flow<ReQuoteTrigger> {
        return withFlowScope { scope ->
            val exchangeRegistry = exchangeRegistry()

            exchangeRegistry.allExchanges()
                .map { it.runSubscriptions() }
                .mergeIfMultiple()
        }
            .debounce(500.milliseconds)
            .inBackground()
    }

    context(scope: ComputationalScope)
    private suspend fun SwapQuoteArgs.calculatePriceImpact(amountIn: Balance, amountOut: Balance): Fraction {
        val priceAssetIn = getCachedPriceUse.getPrice(assetIn)
        val priceAssetOut = getCachedPriceUse.getPrice(assetOut)

        val fiatIn = priceAssetIn.priceOf(assetIn.amountFromPlanks(amountIn))
        val fiatOut = priceAssetOut.priceOf(assetOut.amountFromPlanks(amountOut))

        return calculatePriceImpact(fiatIn.amountPrice, fiatOut.amountPrice)
    }

    private fun QuotedTrade.amountIn(): Balance {
        return when (direction) {
            SwapDirection.SPECIFIED_IN -> firstSegmentQuotedAmount
            SwapDirection.SPECIFIED_OUT -> firstSegmentQuote
        }
    }

    private fun QuotedTrade.amountOut(): Balance {
        return when (direction) {
            SwapDirection.SPECIFIED_IN -> lastSegmentQuote
            SwapDirection.SPECIFIED_OUT -> lastSegmentQuotedAmount
        }
    }

    private fun QuotedTrade.finalQuote(): Balance {
        return when (direction) {
            SwapDirection.SPECIFIED_IN -> lastSegmentQuote
            SwapDirection.SPECIFIED_OUT -> firstSegmentQuote
        }
    }

    private fun calculatePriceImpact(fiatIn: BigDecimal, fiatOut: BigDecimal): Fraction {
        if (fiatIn.isZero() || fiatOut.isZero()) return Fraction.ZERO

        val priceImpact = (BigDecimal.ONE - fiatOut / fiatIn).atLeastZero()

        return priceImpact.asFraction
    }

    context(scope: ComputationalScope)
    private fun directionsGraph(): Flow<SwapGraph> {
        return directionsGraphState()
            .map { it.graph }
    }

    context(scope: ComputationalScope)
    private fun directionsGraphState(): Flow<SwapGraphState> {
        return computationalCache.useSharedFlow(ALL_DIRECTIONS_CACHE) {
            val exchangeRegistry = exchangeRegistry()

            val directionsByExchange = exchangeRegistry.allExchanges().map { exchange ->
                flowOf { exchange.availableDirectSwapConnections() }
                    .catch {
                        emit(emptyList())

                        Timber.e(it, "Failed to fetch directions for exchange ${exchange::class.simpleName}")
                    }
            }

            directionsByExchange
                .accumulateLists()
                .filter { it.isNotEmpty() }
                .withIndex()
                .map { (idx, directions) ->
                    val graph = Graph.create(directions)
                    // Every exchange has to emit its directions
                    val fullyLoaded = idx >= directionsByExchange.size - 1

                    SwapGraphState(graph, fullyLoaded)
                }
                .onEach { printGraphStats(it.graph) }
        }
    }

    private suspend fun printGraphStats(graph: SwapGraph) {
        if (!BuildConfig.DEBUG) return

        val allEdges = graph.numberOfEdges()
        val edgesByType = graph.allEdges().groupBy { it::class.simpleName }
        val edgesByTypeStats = edgesByType.entries.joinToString { (type, typeEdges) ->
            "$type: ${typeEdges.size}"
        }

        val message = """
            === Swap Graph Stats ===
            All swap directions: $allEdges
            $edgesByTypeStats
            === Swap Graph Stats ===
        """.trimIndent()

        Timber.d(message)

        graphDebugger.logGraphDiagram(graph)
    }

    context(computationalScope: ComputationalScope)
    private suspend fun exchangeRegistry(): ExchangeRegistry {
        return computationalCache.useCache(EXCHANGES_CACHE) { scope ->
            createExchangeRegistry(scope)
        }
    }

    context(computationalScope: ComputationalScope)
    private suspend fun nodeVisitFilter(): NodeVisitFilter {
        return computationalCache.useCache(NODE_VISIT_FILTER) { scope ->
            NodeVisitFilter(
                computationScope = scope,
                chainsById = chainRegistry.chainsById(),
            )
        }
    }

    private suspend fun createExchangeRegistry(coroutineScope: ComputationalScope): ExchangeRegistry {
        return ExchangeRegistry(
            singleChainExchanges = createIndividualChainExchanges(coroutineScope),
            multiChainExchanges = listOf(
                crossChainTransferFactory.create(createInnerSwapHost(coroutineScope))
            ),
            scope = coroutineScope
        )
    }

    private suspend fun createIndividualChainExchanges(coroutineScope: ComputationalScope): Map<ChainId, AssetExchange> {
        val host = createInnerSwapHost(coroutineScope)

        return chainRegistry.chainsById().mapValues { (_, chain) ->
            createSingleExchange(chain, host)
        }
            .filterNotNull()
    }

    private suspend fun createSingleExchange(
        chain: Chain,
        host: AssetExchange.SwapHost
    ): AssetExchange? {
        val factory = when (chain.id) {
            knownChains.hydration -> hydraDxExchangeFactory
            knownChains.assetHub -> assetConversionFactory
            else -> null
        }

        return factory?.create(chain, host)
    }

    private suspend fun createInnerSwapHost(computationScope: ComputationalScope): InnerSwapHost {
        val subscriptions = sharedSwapSubscriptions(computationScope)
        return InnerSwapHost(computationScope, subscriptions)
    }

    private suspend fun sharedSwapSubscriptions(computationScope: CoroutineScope): SharedSwapSubscriptions {
        return computationalCache.useCache(SHARED_SUBSCRIPTIONS, computationScope) {
            RealSharedSwapSubscriptions(computationScope)
        }
    }

    // Assumes each flow will have only single element
    private fun <T> List<Flow<List<T>>>.accumulateLists(): Flow<List<T>> {
        return mergeIfMultiple()
            .runningFold(emptyList()) { acc, directions -> acc + directions }
    }

    context(scope: ComputationalScope)
    private suspend fun quoteTrade(
        chainAssetIn: Chain.Asset,
        chainAssetOut: Chain.Asset,
        amount: Balance,
        swapDirection: SwapDirection,
        logQuotes: Boolean = true
    ): QuotedTrade {
        val quoter = getPathQuoter()

        val bestPathQuote = quoter.findBestPath(chainAssetIn, chainAssetOut, amount, swapDirection)
        if (debug && logQuotes) {
            logQuotes(bestPathQuote.candidates)
        }

        return bestPathQuote.bestPath
    }

    context(computationalScope: ComputationalScope)
    private suspend fun getPathQuoter(): PathQuoter<SwapGraphEdge> {
        return computationalCache.useCache(QUOTER_CACHE) { scope ->
            val graphFlow = directionsGraph()
            val filter = nodeVisitFilter()

            quoterFactory.create(graphFlow, scope, SwapPathFeeEstimator(), filter)
        }
    }

    private inner class SwapPathFeeEstimator : PathFeeEstimator<SwapGraphEdge> {
        override suspend fun roughlyEstimateFee(path: Path<QuotedEdge<SwapGraphEdge>>): PathRoughFeeEstimation {
            // USDT is used to determine usd to selected currency rate without making a separate request to price api
            val usdtOnAssetHub = chainRegistry.getUSDTOnAssetHub() ?: return PathRoughFeeEstimation.zero()

            val operationPrototypes = path.constructAtomicOperationPrototypes()

            val nativeAssetsSegments = operationPrototypes.allNativeAssets()
            val assetIn = chainRegistry.asset(path.first().edge.from)
            val assetOut = chainRegistry.asset(path.last().edge.to)

            val prices = getTokens(assetIn = assetIn, assetOut = assetOut, usdTiedAsset = usdtOnAssetHub, fees = nativeAssetsSegments)

            val totalFiat = operationPrototypes.estimateTotalFeeInFiat(prices, usdtOnAssetHub.fullId)

            return PathRoughFeeEstimation(
                inAssetIn = prices.fiatToPlanks(totalFiat, assetIn),
                inAssetOut = prices.fiatToPlanks(totalFiat, assetOut)
            )
        }

        private suspend fun ChainRegistry.getUSDTOnAssetHub(): Chain.Asset? {
            val assetHub = getChain(this@RealSwapService.knownChains.assetHub)
            return assetHub.assets.find { it.symbol == "USDT" }
        }

        private fun PriceLookup.fiatToPlanks(fiat: BigDecimal, chainAsset: Chain.Asset): Balance {
            val price = get(chainAsset.fullId)
            return price.amountOf(fiat).let(chainAsset::planksFromAmount)
        }

        private suspend fun getTokens(
            assetIn: Chain.Asset,
            assetOut: Chain.Asset,
            usdTiedAsset: Chain.Asset,
            fees: List<Chain.Asset>
        ): PriceLookup {
            val allTokensToRequestPrices = buildList {
                addAll(fees)
                add(assetIn)
                add(usdTiedAsset)
                add(assetOut)
            }

            return getPriceUse.getPrices(allTokensToRequestPrices)
        }

        private suspend fun List<AtomicSwapOperationPrototype>.allNativeAssets(): List<Chain.Asset> {
            return map {
                val chain = chainRegistry.getChain(it.fromChain)
                chain.utilityAsset
            }
        }

        private suspend fun List<AtomicSwapOperationPrototype>.estimateTotalFeeInFiat(
            prices: PriceLookup,
            usdTiedAsset: FullChainAssetId
        ): BigDecimal {
            return sumOf {
                val nativeAssetId = it.fromChain.utilityAssetId
                val price = prices[nativeAssetId]

                val usdConverter = PriceBasedUsdConverter(prices, nativeAssetId, usdTiedAsset)

                val roughFee = it.roughlyEstimateNativeFee(usdConverter)
                price.priceOf(roughFee).amountPrice
            }
        }

        private inner class PriceBasedUsdConverter(
            private val prices: PriceLookup,
            private val nativeAsset: FullChainAssetId,
            private val usdTiedAsset: FullChainAssetId,
        ) : UsdConverter {
            val currencyToUsdRate = determineCurrencyToUsdRate()

            override suspend fun nativeAssetEquivalentOf(usdAmount: Double): BigDecimal {
                val priceInCurrency = prices[nativeAsset].perUnitPrice ?: return BigDecimal.ZERO
                val priceInUsd = priceInCurrency * currencyToUsdRate
                return usdAmount.toBigDecimal() / priceInUsd
            }

            private fun determineCurrencyToUsdRate(): BigDecimal {
                val usdTiedAssetPrice = prices[usdTiedAsset]
                val rate = usdTiedAssetPrice.perUnitPrice.orZero()
                if (rate.isZero()) return BigDecimal.ZERO

                return BigDecimal.ONE.divide(rate, MathContext.DECIMAL64)
            }
        }
    }

    private inner class InnerSwapHost(
        override val scope: ComputationalScope,
        override val sharedSubscriptions: SharedSwapSubscriptions,
    ) : AssetExchange.SwapHost, ComputationalScope by scope {
        override suspend fun quote(quoteArgs: ParentQuoterArgs): Balance {
            return quoteTrade(
                chainAssetIn = quoteArgs.chainAssetIn,
                chainAssetOut = quoteArgs.chainAssetOut,
                amount = quoteArgs.amount,
                swapDirection = quoteArgs.swapDirection,
                logQuotes = false
            ).finalQuote()
        }

        override suspend fun getFeePaymentProvider(): FeePaymentProvider {
            return exchangeRegistry().feePaymentProvider.await()
        }
    }

    private fun logFee(fee: SwapFee) {
        val route = fee.segments.joinToString(separator = "\n") { segment ->
            val allFees = buildList {
                add(segment.fee.submissionFee)
                addAll(segment.fee.postSubmissionFees.paidByAccount)
                addAll(segment.fee.postSubmissionFees.paidFromAmount)
            }

            allFees.joinToString { it.toString() }
        }

        Timber.d("---- Fees -----")
        Timber.d(route)
        Timber.d("---- End Fees -----")
    }

    private suspend fun logQuotes(quotedTrades: List<QuotedTrade>) {
        val allCandidates = quotedTrades.sortedDescending()
            .map { trade -> formatTrade(trade) }
            .joinToString(separator = "\n")

        Timber.d("-------- New quote ----------")
        Timber.d(allCandidates)
        Timber.d("-------- Done quote ----------\n\n\n")
    }

    private suspend fun formatTrade(trade: QuotedTrade): String {
        return buildString {
            val weightBreakdown = WeightBreakdown.fromQuotedPath(trade)

            trade.path.zip(weightBreakdown.individualWeights).onEachIndexed { index, (quotedSwapEdge, weight) ->
                val amountIn: Balance
                val amountOut: Balance

                when (trade.direction) {
                    SwapDirection.SPECIFIED_IN -> {
                        amountIn = quotedSwapEdge.quotedAmount
                        amountOut = quotedSwapEdge.quote
                    }

                    SwapDirection.SPECIFIED_OUT -> {
                        amountIn = quotedSwapEdge.quote
                        amountOut = quotedSwapEdge.quotedAmount
                    }
                }

                if (index == 0) {
                    val assetIn = chainRegistry.asset(quotedSwapEdge.edge.from)
                    val initialAmount = assetIn.debugFormatBalance(amountIn)
                    append(initialAmount)

                    if (trade.direction == SwapDirection.SPECIFIED_OUT) {
                        val roughFeesInAssetIn = trade.roughFeeEstimation.inAssetIn
                        val roughFeesInAssetInAmount = assetIn.debugFormatBalance(roughFeesInAssetIn)

                        append(" (+$roughFeesInAssetInAmount fees) ")
                    }
                }

                append(" --- ${quotedSwapEdge.edge.debugLabel()} (w: $weight)---> ")

                val assetOut = chainRegistry.asset(quotedSwapEdge.edge.to)
                val outAmount = assetOut.debugFormatBalance(amountOut)

                append(outAmount)

                if (index == trade.path.size - 1) {
                    if (trade.direction == SwapDirection.SPECIFIED_IN) {
                        val roughFeesInAssetOut = trade.roughFeeEstimation.inAssetOut
                        val roughFeesInAssetOutAmount = assetOut.debugFormatBalance(roughFeesInAssetOut)

                        append(" (-$roughFeesInAssetOutAmount fees, w: ${weightBreakdown.total})")
                    }
                }
            }
        }
    }

    private fun Chain.Asset.debugFormatBalance(planks: Balance): String {
        return amountFormatter.formatTokenAmount(
            tokenAmount = tokenAmountMapper.mapFrom(withAmount(planks)),
            precision = RoundPrecision.DEFAULT
        )
    }

    private inner class ExchangeRegistry(
        private val singleChainExchanges: Map<ChainId, AssetExchange>,
        private val multiChainExchanges: List<AssetExchange>,
        scope: CoroutineScope,
    ) {
        val feePaymentProvider by scope.lazyAsync {
            createFeePaymentRegistry()
        }

        private suspend fun createFeePaymentRegistry(): FeePaymentProvider {
            val overrides = allExchanges().flatMap { it.feePaymentOverrides() }
                .associateBy(
                    keySelector = { it.chainId },
                    valueTransform = { it.provider }
                )

            return FeePaymentRegistry(overrides)
        }

        fun allExchanges(): List<AssetExchange> {
            return buildList {
                addAll(singleChainExchanges.values)
                addAll(multiChainExchanges)
            }
        }
    }

    /**
     * Check that it is possible to pay fees in moving asset
     */
    private inner class NodeVisitFilter(
        val computationScope: ComputationalScope,
        val chainsById: ChainsById,
    ) : EdgeVisitFilter<SwapGraphEdge>, ComputationalScope by computationScope {
        private val feePaymentCapabilityCache = MapCache<ChainId, FastLookupCustomFeeCapability?>(computationScope) { chainId ->
            createFastLookupFeeCapability(chainId)
        }

        suspend fun warmUpChain(chainId: ChainId) {
            feePaymentCapabilityCache.getOrCompute(chainId)
        }

        override suspend fun shouldVisit(edge: SwapGraphEdge, pathPredecessor: SwapGraphEdge?): Boolean {
            val chainAndAssetOut = chainsById.chainWithAssetOrNull(edge.to) ?: return false

            // First path segments don't have any extra restrictions
            if (pathPredecessor == null) return true

            // We don't (yet) handle edges that doesn't allow to transfer whole account balance out
            if (!edge.canTransferOutWholeAccountBalance()) return false

            // Destination asset must be sufficient
            if (!isSufficient(chainAndAssetOut)) return false

            val chainAndAssetIn = chainsById.chainWithAssetOrNull(edge.from) ?: return false

            // Since we allow insufficient asset out in paths with length 1, we want to reject paths with length > 1
            // by checking sufficiency of assetIn (which was assetOut in the previous segment)
            if (!isSufficient(chainAndAssetIn)) return false

            // Besides checks above, utility assets don't have any other restrictions
            if (edge.from.isUtility) return true

            // Edge might request us to ignore the default requirement based on its direct predecessor
            if (edge.predecessorHandlesFees(pathPredecessor)) return true

            val feeCapability = feePaymentCapabilityCache.getOrCompute(edge.from.chainId)

            return feeCapability != null && feeCapability.canPayFeeInNonUtilityToken(edge.from.assetId) &&
                edge.canPayNonNativeFeesInIntermediatePosition()
        }

        private fun isSufficient(chainAndAsset: ChainWithAsset): Boolean {
            return assetSourceRegistry.typeFor(chainAndAsset.asset).isSelfSufficient()
        }

        private suspend fun createFastLookupFeeCapability(chainId: ChainId): FastLookupCustomFeeCapability? {
            val feePaymentRegistry = exchangeRegistry().feePaymentProvider()
            return feePaymentRegistry.feeCapabilityLookup(chainId)
                .logFailure("Failed to construct fast custom fee lookup for chain $chainId")
                .getOrNull()
        }
    }

    private inner class RealSharedSwapSubscriptions(
        private val coroutineScope: CoroutineScope,
    ) : SharedSwapSubscriptions, CoroutineScope by coroutineScope {
        private val blockNumberCache = SharedFlowMapCache<ChainId, BlockNumber>(coroutineScope) { chainId ->
            chainStateRepository.currentRemoteBlockNumberFlow(chainId, null)
        }

        override suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber> {
            return blockNumberCache.getOrCompute(chainId)
        }
    }

    private class SwapGraphState(val graph: SwapGraph, val fullyLoaded: Boolean)
}

private typealias QuotedTrade = QuotedPath<SwapGraphEdge>
